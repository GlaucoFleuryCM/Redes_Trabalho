use std::io::Write;
use std::net::TcpStream;
use std::{thread, time::Duration};

use crate::componentes::env_io;
use crate::protocolo::protocolo::{Connect, EncodeDecode, Header, Mensagem, Payload, SensorData, TipoMensagem};

const TEMP_FILE: &str = "src/env_vars/temp.txt";
const HUM_FILE: &str = "src/env_vars/hum.txt";
const CO2_FILE: &str = "src/env_vars/co2.txt";
const SERVER_ADDR: &str = "127.0.0.1:8080";
const SLEEP_DURATION_S: u64 = 2;

pub struct Sensor {
    id: u8,
    file_path: String,
}

impl Sensor {
    pub fn new(id: u8) -> Self {
        let file_path = match id {
            0 => TEMP_FILE.to_string(),
            1 => HUM_FILE.to_string(),
            2 => CO2_FILE.to_string(),
            _ => panic!("ID de sensor inválido"),
        };
        Sensor { id, file_path }
    }

    pub fn start(&self) {
        let id = self.id;
        let file_path = self.file_path.clone();
        
        thread::spawn(move || {
            // Tenta conectar ao servidor
            let mut stream = loop {
                match TcpStream::connect(SERVER_ADDR) {
                    Ok(stream) => {
                        println!("Sensor {}: Conectado ao servidor.", id);
                        break stream;
                    }
                    Err(_) => {
                        eprintln!("Sensor {}: Falha ao conectar. Tentando novamente em 5s.", id);
                        thread::sleep(Duration::from_secs(5));
                    }
                }
            };

            // Envia mensagem de Connect
            let connect_payload = Connect { tipo: 0, id }; // tipo 0 para sensor
            let connect_msg = Mensagem {
                header: Header {
                    magic_number: u32::from_be_bytes(*b"PPPP"),
                    versao: 1,
                    ack: false,
                    reserved: 0,
                    tipo: TipoMensagem::CONNECT,
                    tamanho: connect_payload.encode().len() as u16,
                },
                payload: Some(Payload::Connect(connect_payload)),
            };

            if let Err(e) = stream.write_all(&connect_msg.encode()) {
                eprintln!("Sensor {}: Falha ao enviar mensagem de conexão: {}", id, e);
                return;
            }
            println!("Sensor {}: Mensagem de conexão enviada.", id);

            // Loop principal de leitura e envio
            loop {
                match env_io::read_value(&file_path) {
                    Ok(value) => {
                        let data_payload = SensorData {
                            sensor_id: id as u32,
                            value,
                        };
                        let data_msg = Mensagem {
                            header: Header {
                                magic_number: u32::from_be_bytes(*b"PPPP"),
                                versao: 1,
                                ack: false,
                                reserved: 0,
                                tipo: TipoMensagem::SENSOR_DATA,
                                tamanho: data_payload.encode().len() as u16,
                            },
                            payload: Some(Payload::SensorData(data_payload)),
                        };

                        if let Err(e) = stream.write_all(&data_msg.encode()) {
                            eprintln!("Sensor {}: Falha ao enviar dados para o servidor: {}", id, e);
                            // Tenta reconectar
                            stream = loop {
                                match TcpStream::connect(SERVER_ADDR) {
                                    Ok(s) => {
                                        println!("Sensor {}: Reconectado ao servidor.", id);
                                        break s;
                                    }
                                    Err(_) => {
                                        eprintln!("Sensor {}: Falha ao reconectar. Tentando novamente em 5s.", id);
                                        thread::sleep(Duration::from_secs(5));
                                    }
                                }
                            };
                        } else {
                            println!("Sensor {}: Enviou o valor {:.2}", id, value);
                        }
                    }
                    Err(e) => eprintln!("Sensor {}: Erro ao ler o valor do arquivo: {}", id, e),
                }

                thread::sleep(Duration::from_secs(SLEEP_DURATION_S));
            }
        });
    }
}
