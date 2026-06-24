use std::io::{Read, Write};
use std::net::TcpStream;
use std::{thread, time::Duration};

use crate::protocolo::protocolo::{Config, EncodeDecode, Header, Mensagem, Payload, SensorQuery, TipoMensagem};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const QUERY_INTERVAL_S: u64 = 5;

pub struct Cliente {}

impl Cliente {
    pub fn new() -> Self {
        Cliente {}
    }

    pub fn run(&self) {
        println!("Cliente: Iniciando.");
        // Tenta conectar ao servidor
        let mut stream = loop {
            match TcpStream::connect(SERVER_ADDR) {
                Ok(stream) => {
                    println!("Cliente: Conectado ao servidor.");
                    break stream;
                }
                Err(_) => {
                    eprintln!("Cliente: Falha ao conectar. Tentando novamente em 5s.");
                    thread::sleep(Duration::from_secs(5));
                }
            }
        };

        // Envia as configurações iniciais
        self.send_initial_config(&mut stream);

        // Dá um tempo para o gerenciador processar as configs
        thread::sleep(Duration::from_secs(1));

        // Loop para fazer queries 3 vezes
        for _ in 0..3 {
            self.query_and_print_sensors(&mut stream);
            thread::sleep(Duration::from_secs(QUERY_INTERVAL_S));
        }
    }

    fn send_config(&self, stream: &mut TcpStream, key: u8, value: f32) {
        let config_payload = Config { key, value };
        let config_msg = Mensagem {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                versao: 1,
                ack: false,
                reserved: 0,
                tipo: TipoMensagem::CONFIG,
                tamanho: config_payload.encode().len() as u16,
            },
            payload: Some(Payload::Config(config_payload)),
        };

        if let Err(e) = stream.write_all(&config_msg.encode()) {
            eprintln!("Cliente: Falha ao enviar configuração ({}, {}): {}", key, value, e);
        } else {
            // Pequeno delay para não sobrecarregar o servidor com muitas mensagens de uma vez
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn send_initial_config(&self, stream: &mut TcpStream) {
        println!("Cliente: Enviando configurações iniciais para o Gerenciador.");
        // (min_temp, max_temp, his_temp, min_hum, his_hum, min_co2, his_co2)
        // IDs: (0,      1,        2,        3,       5,       6,       8)
        self.send_config(stream, 0, 20.0); // min_temp
        self.send_config(stream, 1, 30.0); // max_temp
        self.send_config(stream, 2, 1.0);  // his_temp
        self.send_config(stream, 3, 40.0); // min_hum
        self.send_config(stream, 5, 5.0);  // his_hum
        self.send_config(stream, 6, 300.0);// min_co2
        self.send_config(stream, 8, 50.0); // his_co2
        println!("Cliente: Configurações enviadas.");
    }

    fn query_and_print_sensors(&self, stream: &mut TcpStream) {
        let mut buffer = [0; 1024];
        for sensor_id in 0..=2 {
            let query_payload = SensorQuery { sensor_id: sensor_id as u32 };
            let query_msg = Mensagem {
                header: Header {
                    magic_number: u32::from_be_bytes(*b"PPPP"),
                    versao: 1,
                    ack: false,
                    reserved: 0,
                    tipo: TipoMensagem::SENSOR_QUERY,
                    tamanho: query_payload.encode().len() as u16,
                },
                payload: Some(Payload::SensorQuery(query_payload)),
            };

            if stream.write_all(&query_msg.encode()).is_err() {
                eprintln!("Cliente: Falha ao enviar query para o sensor {}", sensor_id);
                continue;
            }

            match stream.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    if let Some((msg, _)) = Mensagem::try_decode(&buffer[..size]) {
                        if let Some(Payload::SensorRes(res)) = msg.payload {
                            let sensor_name = match res.sensor_id {
                                0 => "Temperatura",
                                1 => "Umidade",
                                2 => "CO2",
                                _ => "Desconhecido",
                            };
                            println!("Cliente: Valor recebido -> {}: {:.2}", sensor_name, res.value);
                        }
                    }
                }
                Ok(_) => {},
                Err(e) => {
                    eprintln!("Cliente: Erro ao ler resposta do servidor: {}. Tentando reconectar...", e);
                    // Tenta reconectar
                    *stream = loop {
                        match TcpStream::connect(SERVER_ADDR) {
                            Ok(s) => {
                                println!("Cliente: Reconectado ao servidor.");
                                break s;
                            }
                            Err(_) => {
                                eprintln!("Cliente: Falha ao reconectar. Tentando novamente em 5s.");
                                thread::sleep(Duration::from_secs(5));
                            }
                        }
                    };
                    // Reenvia configs após reconectar
                    self.send_initial_config(stream);
                    break; // Sai do loop de query para tentar de novo no próximo ciclo
                }
            }
        }
    }
}

impl Default for Cliente {
    fn default() -> Self {
        Self::new()
    }
}
