/*
File contém: lógica principal do componente Gerenciador;
*/

use crate::protocolo::protocolo::{Mensagem, TipoMensagem, Header, Payload, Config, Connect, SensorData, SensorQuery, ActCmd, SensorRes, EncodeDecode};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use crate::componentes::env_io;
use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

const PORT_SERVER: &str = "127.0.0.1:8080";
const HUM_FILE: &str = "src/env_vars/hum.txt";
const CO2_FILE: &str = "src/env_vars/co2.txt";
const DECAY_SLEEP_DURATION_MS: u64 = 1500;
const HUM_DECAY: f32 = 0.3;
const CO2_DECAY: f32 = 0.6;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Estado {
    Aquecendo,
    Resfriando,
    Desligado,
}

pub struct Gerenciador {
    max_temp: f32,
    min_temp:f32,
    min_hum:f32,
    min_co2:f32,
    his_temp:f32,
    his_hum:f32,
    his_co2:f32,
    setting_flag: bool,
    atuadores: HashSet<u8>,
    sensores: HashSet<u8>,
    atuadores_streams: HashMap<u8, Arc<Mutex<TcpStream>>>,
    curr_temp: f32,
    curr_hum: f32, 
    curr_co2: f32,
    estado_temp: Estado,
    estado_hum: bool,
    estado_co2: bool,
}

impl Default for Gerenciador {
    fn default() -> Gerenciador {
        Self::new()
    }
}

impl Gerenciador {
    pub fn new() -> Gerenciador {
        Gerenciador {
            max_temp: -1.0,
            min_temp: -1.0,
            min_hum: -1.0,
            min_co2: -1.0,
            his_co2: -1.0,
            his_temp: -1.0,
            his_hum: -1.0,
            setting_flag: false,
            atuadores: HashSet::new(),
            sensores: HashSet::new(),
            atuadores_streams: HashMap::new(),
            curr_temp: -1.0,
            curr_hum: -1.0, 
            curr_co2: -1.0,
            estado_temp: Estado::Desligado,
            estado_hum: false,
            estado_co2: false,
        }
    }

    pub fn run(self) {
        let gerenciador_arc = Arc::new(Mutex::new(self));
        let listener = TcpListener::bind(PORT_SERVER).unwrap();
        println!("Gerenciador ouvindo em {}", PORT_SERVER);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("Nova conexão: {}", stream.peer_addr().unwrap());
                    let ger_clone = Arc::clone(&gerenciador_arc);
                    let stream_clone = Arc::new(Mutex::new(stream));
                    thread::spawn(move || {
                        handle_connection(stream_clone, ger_clone);
                    });
                }
                Err(e) => {
                    eprintln!("Erro ao aceitar conexão: {}", e);
                }
            }
        }
    }

    pub fn start_decay_thread(&self) {
        thread::spawn(move || {
            println!("Thread de decaimento do Gerenciador iniciada.");
            loop {
                thread::sleep(Duration::from_millis(DECAY_SLEEP_DURATION_MS));
                match env_io::read_value(HUM_FILE) {
                    Ok(mut val) => {
                        val -= HUM_DECAY;
                        if val < 0.0 { val = 0.0; }
                        if let Err(e) = env_io::write_value(HUM_FILE, val) {
                            eprintln!("Gerenciador: Erro ao escrever no arquivo de umidade: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Gerenciador: Erro ao ler o arquivo de umidade: {}", e),
                }

                match env_io::read_value(CO2_FILE) {
                    Ok(mut val) => {
                        val -= CO2_DECAY;
                        if val < 0.0 { val = 0.0; }
                        if let Err(e) = env_io::write_value(CO2_FILE, val) {
                            eprintln!("Gerenciador: Erro ao escrever no arquivo de CO2: {}", e);
                        }
                    }
                    Err(e) => eprintln!("Gerenciador: Erro ao ler o arquivo de CO2: {}", e),
                }
            }
        });
    }

    fn check_config(&self) -> bool {
        let lista_valores = vec![
            self.max_temp,
            self.min_temp,
            self.min_hum,
            self.min_co2,
            self.his_temp,
            self.his_hum,
            self.his_co2,
        ];
        !lista_valores.iter().any(|&v| v < 0.0)
    }

    fn return_ack(&self, tipo_msg: TipoMensagem) -> Mensagem {
        Mensagem {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                versao: 1,
                ack: true,
                reserved: 0,
                tipo: tipo_msg,
                tamanho: 0,
            },
            payload: None,
        }
    }

    fn controle_temp(&self) -> Estado {
        match self.estado_temp {
            Estado::Desligado => {
                if (self.curr_temp > self.max_temp + self.his_temp) {
                    Estado::Resfriando
                } else if (self.curr_temp < self.min_temp - self.his_temp) {
                    Estado::Aquecendo
                } else {
                    Estado::Desligado
                }
            }
            Estado::Resfriando => {
                if (self.curr_temp <= self.max_temp - self.his_temp) {
                    Estado::Desligado
                } else {
                    Estado::Resfriando
                }
            }
            Estado::Aquecendo => {
                if (self.curr_temp >= self.min_temp + self.his_temp) {
                    Estado::Desligado
                } else {
                    Estado::Aquecendo
                }
            }
        }
    }

    fn handle_connect(&mut self, payload: Connect, stream: Arc<Mutex<TcpStream>>) -> Option<Mensagem> {
        let tipo_dispositivo = payload.tipo;
        let id = payload.id;

        if (tipo_dispositivo != 1) && (tipo_dispositivo != 0) {
            println!("Dispositivo desconhecido; cadastro cancelado...");
            return None;
        }

        if id > 6 {
            println!("ID não reconhecido; cadastro cancelado...");
            return None;
        }

        let dispositivos = vec![
            "Sensor temperatura",        
            "Sensor de umidade",         
            "Sensor de nível de CO2",    
            "Atuador aquecedor",         
            "Atuador resfriador",        
            "Atuador de irrigação",      
            "Atuador injetor de CO2",    
        ];

        match tipo_dispositivo {
            0 => {
                self.sensores.insert(id);
                println!("Sensor cadastrado: {}", id);
                println!("Função do Sensor: {}", dispositivos[id as usize]);
            },
            1 => {
                self.atuadores.insert(id);
                self.atuadores_streams.insert(id, stream);
                println!("Atuador cadastrado: {}", id);
                println!("Função do atuador: {}", dispositivos[id as usize]);
            },
            _ => unreachable!(),
        };

        Some(self.return_ack(TipoMensagem::CONNECT))
    }

    fn send_act_cmd(&self, id_atuador: u8, comando: u8) {
        if let Some(stream_arc) = self.atuadores_streams.get(&id_atuador) {
            let mut stream = stream_arc.lock().unwrap();
            let payload = ActCmd { command: comando };
            let msg = Mensagem {
                header: Header {
                    magic_number: u32::from_be_bytes(*b"PPPP"),
                    versao: 1,
                    ack: false,
                    reserved: 0,
                    tipo: TipoMensagem::ACT_CMD,
                    tamanho: payload.encode().len() as u16,
                },
                payload: Some(Payload::ActCmd(payload)),
            };
            if stream.write_all(&msg.encode()).is_err() {
                eprintln!("Gerenciador: Erro ao enviar comando para o atuador {}", id_atuador);
            }
        }
    }

    fn handle_sensor_data(&mut self, payload: SensorData) -> Option<Mensagem> {
        let sensor_id = payload.sensor_id as u8;
        let leitura = payload.value;

        if self.sensores.get(&sensor_id).is_none() {
            println!("Sensor de id {} não conectado; informação descartada", sensor_id);
            return None;
        }

        match sensor_id {
            0 => { // Sensor de Temperatura
                self.curr_temp = leitura;
                let estado_anterior = self.estado_temp;
                self.estado_temp = self.controle_temp();
                if estado_anterior != self.estado_temp {
                    match self.estado_temp {
                        Estado::Aquecendo => {
                            self.send_act_cmd(3, 1); // Ligar aquecedor
                            self.send_act_cmd(4, 0); // Desligar resfriador
                        },
                        Estado::Resfriando => {
                            self.send_act_cmd(3, 0); // Desligar aquecedor
                            self.send_act_cmd(4, 1); // Ligar resfriador
                        },
                        Estado::Desligado => {
                            self.send_act_cmd(3, 0); // Desligar aquecedor
                            self.send_act_cmd(4, 0); // Desligar resfriador
                        }
                    }
                }
            },
            1 => { // Sensor de Umidade
                self.curr_hum = leitura;
                let estado_anterior = self.estado_hum;
                if self.curr_hum < self.min_hum - self.his_hum {
                    self.estado_hum = true;
                } else if self.curr_hum > self.min_hum + self.his_hum {
                    self.estado_hum = false;
                }
                if estado_anterior != self.estado_hum {
                    self.send_act_cmd(5, self.estado_hum as u8); // Ligar/desligar irrigador
                }
            },
            2 => { // Sensor de CO2
                self.curr_co2 = leitura;
                let estado_anterior = self.estado_co2;
                if self.curr_co2 < self.min_co2 - self.his_co2 {
                    self.estado_co2 = true;
                } else if self.curr_co2 > self.min_co2 + self.his_co2 {
                    self.estado_co2 = false;
                }

                if estado_anterior != self.estado_co2 {
                    self.send_act_cmd(6, self.estado_co2 as u8); // Ligar/desligar injetor de CO2
                }
            },
            _ => {}
        };

        Some(self.return_ack(TipoMensagem::SENSOR_DATA))
    }

    fn handle_act_cmd(&mut self, payload: ActCmd) -> Option<Mensagem> {
        None
    }

    fn handle_sensor_query(&mut self, payload: SensorQuery) -> Option<Mensagem> {
        let id_query = payload.sensor_id as u8;
        let valor_sensor = match id_query {
            0 => self.curr_temp,
            1 => self.curr_hum,
            2 => self.curr_co2,
            _ => {
                println!("Recebido SENSOR_QUERY para um ID de sensor desconhecido: {}", id_query);
                return None;
            }
        };

        let payload_res = SensorRes { sensor_id: id_query as u32, value: valor_sensor };
        Some(Mensagem {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                versao: 1,
                ack: false,
                reserved: 0,
                tipo: TipoMensagem::SENSOR_RES,
                tamanho: payload_res.encode().len() as u16,
            },
            payload: Some(Payload::SensorRes(payload_res)),
        })
    }

    fn handle_sensor_res(&mut self, payload: SensorRes) -> Option<Mensagem> {
        None
    }
    fn handle_config(&mut self, payload: Config) -> Option<Mensagem> {
        let atualizacao = payload.value;
        let id = payload.key;
        let nome_campo = match id {
            0 => { self.min_temp = atualizacao; "min_temp" },
            1 => { self.max_temp = atualizacao; "max_temp" },
            2 => { self.his_temp = atualizacao; "his_temp" },
            3 => { self.min_hum = atualizacao; "min_hum" },
            5 => { self.his_hum = atualizacao; "his_hum" },
            6 => { self.min_co2 = atualizacao; "min_co2" },
            8 => { self.his_co2 = atualizacao; "his_co2" },
            _ => {
                println!("ID inválido: {}", id);
                return None;
            }
        };
        println!("Atualização do Gerenciador: {} -> {}", nome_campo, atualizacao);

        let prev: bool = self.setting_flag;
        if !prev {
            self.setting_flag = self.check_config();
            if self.setting_flag {
                println!("Gerenciador 100% configurado e pronto p/uso!");
            }
        }

        Some(self.return_ack(TipoMensagem::CONFIG))
    }

    fn interpretar_mensagem(&mut self, mensagem: Mensagem, stream: Option<Arc<Mutex<TcpStream>>>) -> Option<Mensagem> {
        // Valida magic
        if mensagem.header.magic_number != u32::from_be_bytes(*b"PPPP") {
            println!("Protocolo não registrado; use PPPP");
            return None;
        }

        // Gate de configuração
        if (mensagem.header.tipo != TipoMensagem::CONFIG) && !self.setting_flag {
            println!("Servidor ainda não configurado; Mensagens fora CONFIG serão ignoradas");
            return None;
        }

        if let Some(payload) = mensagem.payload {
            match payload {
                Payload::Connect(p) => {
                    if let Some(stream) = stream {
                        return self.handle_connect(p, stream);
                    }
                    return None;
                },
                Payload::SensorData(p) => return self.handle_sensor_data(p),
                Payload::ActCmd(p) => return self.handle_act_cmd(p),
                Payload::SensorQuery(p) => return self.handle_sensor_query(p),
                Payload::SensorRes(p) => return self.handle_sensor_res(p),
                Payload::Config(p) => return self.handle_config(p),
            }
        }

        // sem payload
        println!("Mensagem recebida sem payload ou não identificada;");
        None
    }
}

fn handle_connection(stream_arc: Arc<Mutex<TcpStream>>, ger_arc: Arc<Mutex<Gerenciador>>) {
    let mut receive_buffer = Vec::new();
    loop {
        let mut temp_buffer = [0u8; 1024];
        let mut stream = stream_arc.lock().unwrap();
        match stream.read(&mut temp_buffer) {
            Ok(0) => {
                println!("Conexão fechada pelo peer.");
                break;
            }
            Ok(size) => {
                receive_buffer.extend_from_slice(&temp_buffer[..size]);
                while let Some((mensagem, consumed)) = Mensagem::try_decode(&receive_buffer) {
                    let mut ger = ger_arc.lock().unwrap();
                    if let Some(response) = ger.interpretar_mensagem(mensagem, Some(stream_arc.clone())) {
                        if stream.write_all(&response.encode()).is_err() {
                            eprintln!("Erro ao enviar resposta.");
                            break;
                        }
                    }
                    receive_buffer.drain(0..consumed);
                }
            }
            Err(_) => {
                println!("Conexão terminada.");
                break;
            }
        }
    }
}