use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::{thread, time::Duration};

use crate::componentes::env_io;
use crate::protocolo::protocolo::{ActCmd, Connect, EncodeDecode, Header, Mensagem, Payload, TipoMensagem};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const TEMP_FILE: &str = "src/env_vars/temp.txt";
const HUM_FILE: &str = "src/env_vars/hum.txt";
const CO2_FILE: &str = "src/env_vars/co2.txt";

const TEMP_CHANGE: f32 = 0.5;
const HUM_CHANGE: f32 = 1.0;
const CO2_CHANGE: f32 = 2.0;
const ACTION_INTERVAL_MS: u64 = 500;

#[derive(Clone, Copy, Debug)]
pub enum AtuadorTipo {
    Aquecedor,
    Resfriador,
    Irrigador,
    InjetorCO2,
}

pub struct Atuador {
    id: u8,
    tipo: AtuadorTipo,
    active: Arc<Mutex<bool>>,
}

impl Atuador {
    pub fn new(id: u8, tipo: AtuadorTipo) -> Self {
        Atuador {
            id,
            tipo,
            active: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start(&self) {
        let id = self.id;
        let active_clone_listener = Arc::clone(&self.active);
        let active_clone_worker = Arc::clone(&self.active);
        let tipo_clone = self.tipo;

        // Thread para ouvir comandos do gerenciador
        thread::spawn(move || {
            println!("Atuador {} ({:?}): Iniciando thread de escuta.", id, tipo_clone);
            let mut stream = loop {
                match TcpStream::connect(SERVER_ADDR) {
                    Ok(stream) => {
                        println!("Atuador {}: Conectado ao servidor.", id);
                        break stream;
                    }
                    Err(_) => {
                        eprintln!("Atuador {}: Falha ao conectar. Tentando novamente em 5s.", id);
                        thread::sleep(Duration::from_secs(5));
                    }
                }
            };

            // Envia mensagem de Connect - tipo 1 para atuador (corrigido conforme spec)
            let connect_payload = Connect { tipo: 1, id };
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
                eprintln!("Atuador {}: Falha ao enviar mensagem de conexão: {}", id, e);
                return; // Encerra a thread se não conseguir nem conectar
            }
            println!("Atuador {}: Mensagem de conexão enviada.", id);
            
            // Loop para receber comandos
            let mut buffer = [0u8; 1024];
            loop {
                match stream.read(&mut buffer) {
                    Ok(size) if size > 0 => {
                        if let Some((msg, _)) = Mensagem::try_decode(&buffer[..size]) {
                            if let Some(Payload::ActCmd(ActCmd { command })) = msg.payload {
                                let mut active = active_clone_listener.lock().unwrap();
                                *active = command == 1;
                                println!("Atuador {}: Recebeu comando para {}.", id, if *active { "LIGAR" } else { "DESLIGAR" });
                            }
                        }
                    }
                    Ok(_) => { // Conexão fechada
                        println!("Atuador {}: Conexão fechada pelo servidor.", id);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Atuador {}: Erro de leitura: {}. Encerrando.", id, e);
                        break;
                    }
                }
            }
        });

        // Thread para executar a ação do atuador
        thread::spawn(move || {
            println!("Atuador {} ({:?}): Iniciando thread de trabalho.", id, tipo_clone);
            loop {
                let is_active = *active_clone_worker.lock().unwrap();
                if is_active {
                    let (file, change) = match tipo_clone {
                        AtuadorTipo::Aquecedor => (TEMP_FILE, TEMP_CHANGE),
                        AtuadorTipo::Resfriador => (TEMP_FILE, -TEMP_CHANGE),
                        AtuadorTipo::Irrigador => (HUM_FILE, HUM_CHANGE),
                        AtuadorTipo::InjetorCO2 => (CO2_FILE, CO2_CHANGE),
                    };

                    match env_io::read_value(file) {
                        Ok(mut val) => {
                            val += change;
                            if let Err(e) = env_io::write_value(file, val) {
                                eprintln!("Atuador {}: Erro ao escrever no arquivo ({}): {}", id, file, e);
                            }
                        }
                        Err(e) => eprintln!("Atuador {}: Erro ao ler o arquivo ({}): {}", id, file, e),
                    }
                }
                thread::sleep(Duration::from_millis(ACTION_INTERVAL_MS));
            }
        });
    }
}
