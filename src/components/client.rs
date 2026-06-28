use std::io::{Read, Write};
use std::net::TcpStream;
use std::{thread, time::Duration};

use crate::components::{devices, utils};
use crate::protocol::protocol::{Config, EncodeDecode, Message, Payload, SensorQuery, MessageType};

const SERVER_ADDR: &str = "127.0.0.1:8080";
const QUERY_INTERVAL_S: u64 = 5;

pub struct Client {}

impl Client {
    pub fn new() -> Self {
        Client {}
    }

    pub fn run(&self) {
        println!("Cliente: Iniciando.");
        let mut stream = utils::connect(SERVER_ADDR, "cliente");

        // envia as configurações iniciais
        self.send_initial_config(&mut stream);

        // dá um tempo pro gerenciador processar as configs
        thread::sleep(Duration::from_secs(1));

        // loop pra fazer as queries 3 vezes
        for _ in 0..3 {
            self.query_and_print_sensors(&mut stream);
            thread::sleep(Duration::from_secs(QUERY_INTERVAL_S));
        }
    }

    fn send_config(&self, stream: &mut TcpStream, key: u8, value: f32) {
        let config_payload = Config { key, value };
        let config_msg = Message::new(MessageType::CONFIG, Payload::Config(config_payload));

        if let Err(e) = stream.write_all(&config_msg.encode()) {
            eprintln!("Cliente: Falha ao enviar configuração ({}, {}): {}", key, value, e);
        } else {
            // delayzinho pra não afogar o servidor com várias mensagens de uma vez
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn send_initial_config(&self, stream: &mut TcpStream) {
        println!("Cliente: Enviando configurações iniciais para o Gerenciador.");
        // tabela (key do protocolo, valor), evita repetir send_config oito vezes na mão
        // valores apertados de propósito pra demo: bandas estreitas = liga/desliga frequente.
        // temp usa setpoint único (min==max=25) com histerese 1.3, aí o aquecedor e o
        // resfriador alternam batendo nas duas bordas (25-1.3 e 25+1.3); o 1.3 evita cair
        // exatamente no limite (os valores andam na grade de 0.5) e travar na banda morta
        const CONFIGS: [(u8, f32); 8] = [
            (0, 25.0),  // min_temp
            (1, 25.0),  // max_temp
            (2, 1.3),   // hys_temp
            (3, 49.0),  // min_hum
            (4, 51.0),  // max_hum
            (5, 1.0),   // hys_hum
            (6, 400.0), // min_co2
            (8, 5.0),   // hys_co2
        ];
        for (key, value) in CONFIGS {
            self.send_config(stream, key, value);
        }
        println!("Cliente: Configurações enviadas.");
    }

    fn query_and_print_sensors(&self, stream: &mut TcpStream) {
        let mut buffer = [0; 1024];
        for sensor in devices::SENSORS {
            let sensor_id = sensor.id;
            let query_payload = SensorQuery { sensor_id };
            let query_msg = Message::new(MessageType::SensorQuery, Payload::SensorQuery(query_payload));

            if stream.write_all(&query_msg.encode()).is_err() {
                eprintln!("Cliente: Falha ao enviar query para o sensor {}", sensor_id);
                continue;
            }

            match stream.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    if let Some((msg, _)) = Message::try_decode(&buffer[..size]) {
                        if let Some(Payload::SensorRes(res)) = msg.payload {
                            let sensor_name = devices::name_by_id(res.sensor_id);
                            println!("Cliente: Valor recebido -> {}: {:.2}", sensor_name, res.value);
                        }
                    }
                }
                Ok(_) => {},
                Err(e) => {
                    eprintln!("Cliente: Erro ao ler resposta do servidor: {}. Tentando reconectar...", e);
                    *stream = utils::connect(SERVER_ADDR, "cliente (reconectando)");
                    // reenvia as configs depois de reconectar
                    self.send_initial_config(stream);
                    break; // sai do loop de query pra tentar de novo no próximo ciclo
                }
            }
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
