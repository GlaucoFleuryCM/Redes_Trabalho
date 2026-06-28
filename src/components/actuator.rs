use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};

use crate::components::{devices, env_io, utils};
use crate::protocol::protocol::{ActCmd, Connect, EncodeDecode, Message, Payload, MessageType};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const ACTION_INTERVAL_MS: u64 = 500;

pub struct Actuator {
    id: u8,
    file: &'static str,
    variation: f32,
    active: Arc<Mutex<bool>>,
}

impl Actuator {
    pub fn new(id: u8) -> Self {
        // pega arquivo e taxa do registro central, id desconhecido é erro de programação
        let desc = devices::actuator_by_id(id)
            .unwrap_or_else(|| panic!("ID de atuador inválido: {}", id));
        Actuator {
            id,
            file: desc.file,
            variation: desc.variation,
            active: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&self) {
        let id = self.id;
        let file = self.file;
        let variation = self.variation;
        let name = devices::name_by_id(id); // nome interno pra deixar o log mais claro
        let active_listener = Arc::clone(&self.active);
        let active_worker = Arc::clone(&self.active);

        // thread pra ouvir comandos do gerenciador
        thread::spawn(move || {
            println!("Atuador {} ({}): Iniciando thread de escuta.", id, name);
            let mut stream = utils::connect(SERVER_ADDR, &format!("atuador {}", id));

            // envia connect, tipo 1 pra atuador
            let connect_payload = Connect { kind: 1, id };
            let connect_msg = Message::new(MessageType::CONNECT, Payload::Connect(connect_payload));

            if let Err(e) = stream.write_all(&connect_msg.encode()) {
                eprintln!("Atuador {} ({}): Falha ao enviar mensagem de conexão: {}", id, name, e);
                return; // encerra a thread se não conseguir nem conectar
            }
            println!("Atuador {} ({}): Mensagem de conexão enviada.", id, name);

            // loop pra receber comandos
            let mut buffer = [0u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(size) if size > 0 => {
                        if let Some((msg, _)) = Message::try_decode(&buffer[..size]) {
                            if let Some(Payload::ActCmd(ActCmd { command })) = msg.payload {
                                let mut active = active_listener.lock().unwrap();
                                *active = command == 1;
                                println!("Atuador {} ({}): Recebeu comando para {}.", id, name, if *active { "LIGAR" } else { "DESLIGAR" });
                            }
                        }
                    }
                    Ok(_) => { // conexão fechada
                        println!("Atuador {} ({}): Conexão fechada pelo servidor.", id, name);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Atuador {} ({}): Erro de leitura: {}. Encerrando.", id, name, e);
                        break;
                    }
                }
            }
        });

        // thread pra executar a ação do atuador
        thread::spawn(move || {
            println!("Atuador {} ({}): Iniciando thread de trabalho.", id, name);
            loop {
                let is_active = *active_worker.lock().unwrap();
                if is_active {
                    // aplica a variação do registro no arquivo de ambiente desse atuador
                    match env_io::read_value(file) {
                        Ok(mut value) => {
                            value += variation;
                            if let Err(e) = env_io::write_value(file, value) {
                                eprintln!("Atuador {} ({}): Erro ao escrever no arquivo ({}): {}", id, name, file, e);
                            }
                        }
                        Err(e) => eprintln!("Atuador {} ({}): Erro ao ler o arquivo ({}): {}", id, name, file, e),
                    }
                }
                thread::sleep(Duration::from_millis(ACTION_INTERVAL_MS));
            }
        });
    }
}
