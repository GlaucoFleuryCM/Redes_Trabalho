/*
File contém: lógica principal do componente Gerenciador;
*/

use std::fs::{ File };
use file_locking::{ FileExt };
use crate::protocolo::{Mensagem, TipoMensagem, Header};
use std::collections::HashSet;
use std::net::{TcpListener, TcpStream};
use std::io::Read;
use std::process::Command;

const PORT_SERVER: &str = "127.0.0.1:8080";

#[derive(PartialEq, Eq)]
enum Estado {
    Aquecendo,
    Resfriando,
    Desligado,
}

pub struct Gerenciador{
    // varíaveis de controle da estufa;
    max_temp: f32,
    min_temp:f32,
    min_hum:f32,
    min_co2:f32,
    // variaveis de hiperestese;
    hip_temp:f32,
    hip_hum:f32,
    hip_co2:f32,
    /* flag de 1° setting: identifica se o
    o servidor já foi configurado ou não; */
    setting_flag: bool,
    // guardam id de atuadores e sensores cadastrados;
    atuadores: HashSet<u8>,
    sensores: HashSet<u8>,
    // exibem a leitura mais recente do ambiente;
    curr_temp: f32,
    curr_hum: f32, 
    curr_co2: f32,
    // guardam estados do controle do ambiente;
    estado_temp: Estado,
    estado_hum: bool,
    estado_co2: bool,
}

/* inicialização Default do Gerenciador: não configurado */
impl Default for Gerenciador {
    fn default() -> Gerenciador {
        Gerenciador {
            max_temp: -1,
            min_temp: -1,
            min_hum: -1,
            min_co2: -1,
            hip_co2:-1,
            hip_temp: -1,
            hip_hum: -1,
            setting_flag: false,
            atuadores: HashSet::new(),
            sensores: HashSet::new(),
            curr_temp: -1,
            curr_hum: -1, 
            curr_co2: -1,
            estado_temp: Estado::Desligado,
            estado_hum: false,
            estado_co2: false,
        }
    }
}

impl Gerenciador{
    /* 'detectar_conexao': recebe uma stream TCP, interpreta os dados recebidos,
    e aplica a lógica do gerenciador (conversar com os clintes); */
    fn detectar_conexao(&mut self, stream: TcpStream) {
        // buffer de recepção de mensagens;
        let mut receive_buffer = Vec::new();

        // TODO: implementar checakgem de timeout do sensor 

        // iniciando a stream p/detectar outros componentes;
        println!("** Início da Stream de Conexão do Gerenciador **");
        loop {
            let mut temp = [0u8; 1024];

            match stream.read(&mut temp) {
                Ok(received_size) => {
                    if (received_size == 0) {
                        return
                    }

                    receive_buffer.extend_from_slice(&temp[..received_size]);
                    loop {
                        // TODO: arrumar o decode pra retornar Some (ou seja,
                        // tentar ler e, se nn conseguir, avisar)
                        match Mensagem::try_decode(&receive_buffer) {
                            Some((mensagem, size)) => {
                                self.interpretar_mensagem(mensagem);
                                receive_buffer.drain(0..size);
                            }
                            None => break, 
                        }
                    }
                }
                Err(_) => {
                    println!("** Fim da Stream **");
                    return
                }
            }
        }
    }

    fn check_config(&self) -> bool {
        let lista_valores = vec![
            self.max_temp,
            self.min_temp,
            self.min_hum,
            self.min_co2,
            self.hip_temp,
            self.hip_hum,
            self.hip_co2,
        ];
        for valor in &lista_valores {
            if(*valor == -1.0){return false}
        }
        return true
    }

    fn return_ack(&self, tipo_msg: u8) -> Mensagem {
        let ack = Mensagem {
            header: Header {
                magic_number: u32::from_be_bytes(*b"PPPP"),
                versao: 1,
                ack: true,
                reserved: 0,
                tipo: tipo_msg,
                tamanho: 0,
            },
            payload: None,
        };

        return ack
    }

    fn controle_temp(&self) -> Estado {
        match self.estado_temp {
            Estado::Desligado => {
                if (self.curr_temp > self.max_temp + self.hip_temp) {
                    Estado::Resfriando
                } else if (self.curr_temp < self.min_temp - self.hip_temp) {
                    Estado::Aquecendo
                } else {
                    Estado::Desligado
                }
            }
            Estado::Resfriando => {
                if (self.curr_temp <= self.max_temp - self.hip_temp) {
                    Estado::Desligado
                } else {
                    Estado::Resfriando
                }
            }
            Estado::Aquecendo => {
                if (self.curr_temp >= self.min_temp + self.hip_temp) {
                    Estado::Desligado
                } else {
                    Estado::Aquecendo
                }
            }
        }
    }

    // pode retornar um ACK
    fn interpretar_mensagem(&mut self, mensagem:Mensagem) -> Option<Mensagem> {
        if (mensagem.header.magic_number != u32::from_be_bytes(*b"PPPP")) {
            println!("Protocolo não registrado; use PPPP");
            return None
        }

        if ((mensagem.header.tipo != TipoMensagem::CONFIG) && (!self.setting_flag)){
            println!("Servidor ainda não configurado; Mensagens
                      fora CONFIG do cliente serão ignoradas até configuração");
            return None
        }

        match mensagem.header.tipo {
            TipoMensagem::CONFIG => {
                let atualizacao = mensagem.payload.value;
                let id = mensagem.payload.key;
                let nome_campo = match id {
                    0 => { self.min_temp = atualizacao; "min_temp" },
                    1 => { self.max_temp = atualizacao; "max_temp" },
                    2 => { self.hip_temp = atualizacao; "hip_temp" },
                    3 => { self.min_hum = atualizacao; "min_hum" },
                    5 => { self.hip_hum = atualizacao; "hip_hum" },
                    6 => { self.min_co2 = atualizacao; "min_co2" },
                    8 => { self.hip_co2 = atualizacao; "hip_co2" },
                    _ => {
                        println!("ID inválido: {}", id);
                        return None;
                    }
                };

                println!("Atualização do Gerenciador: {} -> {}", nome_campo, atualizacao);

                let prev:bool = self.setting_flag;
                if (!prev){
                    self.setting_flag = self.check_config();
                    if (self.setting_flag){
                        println!("Gerenciador 100% configurado e pronto p/uso!");
                    }
                }

                return Some(self.return_ack(5))
            },
            TipoMensagem::CONNECT => {
                let tipo_dispositivo = mensagem.payload.tipo;
                let id = mensagem.payload.id;

                if((tipo_dispositivo != 1)&&(tipo_dispositivo != 0)){
                    println!("Dispositivo desconhecido; cadastro cancelado...");
                    return None;
                }

                if (id > 6){
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
                        self.atuadores.insert(id);
                        println!("Atuador cadastrado: {}", id);
                        println!("Função do atuador: {}", dispositivos[id])
                    },
                    1 => {
                        self.sensores.insert(id);
                        println!("Sensor cadastrado: {}", id);
                        println!("Função do Sensor: {}", dispositivos[id])
                    },
                };

                return Some(self.return_ack(0))
            },
            TipoMensagem::SENSOR_DATA => {
                let sensor_id = mensagem.payload.sensor_id;
                let leitura = mensagem.payload.value;

                if (self.sensores.get(&sensor_id).is_none()){
                    println!("Sensor de id {} não conectado; informação
                             descartada", sensor_id);
                    return
                }

                /* atualiza a leitura, e decide se ativa ou não um atuador */
                /* TODO: é melhor detectar e arrumar agora as variáveis e daí mandar o ACK,
                ou só mandar direto o ACK e checar as variáveis em outro lugar? */
                match sensor_id {
                    0 => {
                        self.curr_temp = leitura;
                        let prev_estado:Estado = self.estado_temp;
                        self.estado_temp = self.controle_temp();
                        match prev_estado {
                            Estado::Desligado => {
                                if self.estado_temp == Estado::Aquecendo{/*ligar aquecedor*/}
                                if self.estado_temp == Estado::Resfriando{/*ligar resfriador*/}
                            },
                            Estado::Aquecendo => {
                                if self.estado_temp == Estado::Desligado{/*ligar resfriador*/}
                            },
                            Estado::Resfriando => {
                                if self.estado_temp == Estado::Desligado{/*ligar aquecedor*/}
                            },
                        }
                    },
                    1 => {
                        self.curr_hum = leitura;
                        let prev_estado:bool = self.estado_hum;
                        if (prev_estado){
                            if (self.curr_hum > self.min_hum + self.hip_hum) {
                                self.estado_hum = false;
                                /*desligar humidificador*/
                            }
                        }else{
                            if (self.curr_hum < self.min_hum - self.hip_hum) {
                                self.estado_hum = true;
                                /*ligar humidificador*/
                            }
                        }
                    },
                    2 => {
                        self.curr_co2 = leitura;
                        let prev_estado:bool = self.estado_co2;
                        if (prev_estado){
                            if (self.curr_co2 > self.min_co2 + self.hip_co2) {
                                self.estado_co2 = false;
                                /*desligar c02*/
                            }
                        }else{
                            if (self.curr_co2 < self.min_co2 - self.hip_co2) {
                                self.estado_co2 = true;
                                /*ligar c02*/
                            }
                        }
                    },
                };

                return Some(self.return_ack(1))
            },
            TipoMensagem::ACT_CMD =>{
                /* TODO: falar com o Li sobre o que fazer com esse ACK */ 
            },
            TipoMensagem::SENSOR_QUERY => {
                let id_query:u8 = mensagem.payload.sensor_id;
                
                /* TODO: enviar um SENSOR_RES com o valor de curr requisitado */

                return Some(self.return_ack(3))
            },
            TipoMensagem::SENSOR_RES => {
                /* TODO: implementar um ACK */
            },
            _ => {
                println!("Mensagem recebida não identificada;");
                return
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    // intanciando o Gerenciador;
    let gerenciador = Default::default();

    // criando o servidor do Gerenciador;
    let listener = TcpListener::bind(PORT_SERVER);
    match (listener) {
        Ok(listener) => println!("Gerenciador criado com sucesso!"),
        Err(erro) => panic!("Gerenciador não pode ser criado: {}", erro),
    }
    
    /* Criando os demais processos: 1 atuador e 1 sensor 
    para cada parâmetro (humidade, temperatura, co2) */
    let mut at_hum = Command::new("cargo");
    at_hum.args(["run", "--bin", "atuador", "--", "--hum"]);

    let mut sens_hum = Command::new("cargo");
    sens_hum.args(["run", "--bin", "sensor", "--", "--hum"]);

    let mut at_temp = Command::new("cargo");
    at_temp.args(["run", "--bin", "atuador", "--", "--temp"]);

    let mut sens_temp = Command::new("cargo");
    sens_temp.args(["run", "--bin", "sensor", "--", "--temp"]);

    let mut at_co2 = Command::new("cargo");
    at_co2.args(["run", "--bin", "atuador", "--", "--co2"]);

    let mut sens_co2 = Command::new("cargo");
    sens_co2.args(["run", "--bin", "sensor", "--", "--co2"]);

    // cliente para administrar o Gerenciador;
    let mut cliente = Command::new("cargo");
    cliente.args(["run", "--bin", "cliente"]);

    /* Servidor inicia funcionamento padrão (fim do funcionamento após 5min) */
    for stream in listener.incoming() {
        match (stream) {
            Ok(stream) => gerenciador.detectar_conexao(stream),
            Err(erro) => panic!("Erro ocorreu durante IPC: {}", erro),
        }
    }

    println!("Gerenciador fechado com sucesso!");
}