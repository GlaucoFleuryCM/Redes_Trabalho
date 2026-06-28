use std::io::{Read, Write};
use std::{thread, time::Duration};

use crate::components::{devices, env_io, utils};
use crate::protocol::protocol::{Connect, EncodeDecode, Message, Payload, SensorData, MessageType};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const SLEEP_DURATION_S: u64 = 1;

pub struct Sensor {
    id: u8,
    file_path: String,
}

impl Sensor {
    pub fn new(id: u8) -> Self {
        // pega o arquivo de ambiente do registro central, id desconhecido é erro de programação
        let desc = devices::sensor_by_id(id)
            .unwrap_or_else(|| panic!("ID de sensor inválido: {}", id));
        Sensor { id, file_path: desc.file.to_string() }
    }

    pub fn start(&self) {
        let id = self.id;
        let name = devices::name_by_id(id); // nome interno pra deixar o log mais claro
        let file_path = self.file_path.clone();

        thread::spawn(move || {
            let mut stream = utils::connect(SERVER_ADDR, &format!("sensor {}", id));

            // envia connect, tipo 0 pra sensor
            let connect_payload = Connect { kind: 0, id };
            let connect_msg = Message::new(MessageType::CONNECT, Payload::Connect(connect_payload));

            if let Err(e) = stream.write_all(&connect_msg.encode()) {
                eprintln!("Sensor {}: Falha ao enviar mensagem de conexão: {}", id, e);
                return;
            }
            println!("Sensor {}: Mensagem de conexão enviada.", id);

            // espera o ack do gerenciador antes de mandar dado, o protocolo exige isso
            stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
            let mut ack_buf = [0u8; 16];
            match stream.read(&mut ack_buf) {
                Ok(size) if size > 0 => {
                    if let Some((msg, _)) = Message::try_decode(&ack_buf[..size]) {
                        if msg.header.ack {
                            println!("Sensor {}: ack de connect recebido, iniciando envio de dados.", id);
                        } else {
                            eprintln!("Sensor {}: resposta inesperada ao connect, tentando continuar.", id);
                        }
                    }
                }
                _ => eprintln!("Sensor {}: sem ack de connect (timeout ou erro), tentando continuar.", id),
            }
            stream.set_read_timeout(None).ok(); // volta pro modo bloqueante normal

            // loop principal de leitura e envio
            loop {
                match env_io::read_value(&file_path) {
                    Ok(value) => {
                        let data_payload = SensorData {
                            sensor_id: id,
                            value,
                        };
                        let data_msg = Message::new(MessageType::SensorData, Payload::SensorData(data_payload));

                        if let Err(e) = stream.write_all(&data_msg.encode()) {
                            eprintln!("Sensor {}: Falha ao enviar dados para o servidor: {}", id, e);
                            stream = utils::connect(SERVER_ADDR, &format!("sensor {} (reconectando)", id));
                        } else {
                            println!("Sensor {} ({}): Enviou o valor {:.2}", id, name, value);
                        }
                    }
                    Err(e) => eprintln!("Sensor {}: Erro ao ler o valor do arquivo: {}", id, e),
                }

                thread::sleep(Duration::from_secs(SLEEP_DURATION_S));
            }
        });
    }
}
