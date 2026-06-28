/*
file contém: lógica principal do componente gerenciador;
*/

use crate::protocol::protocol::{Message, MessageType, Payload, Config, Connect, SensorData, SensorQuery, ActCmd, SensorRes, EncodeDecode};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use crate::components::{devices, env_io};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

const PORT_SERVER: &str = "127.0.0.1:8080";
const DECAY_SLEEP_DURATION_MS: u64 = 1500;

// configs padrão pra estufa já nascer pronta pra uso, sem depender do cliente mandar CONFIG;
// o cliente ainda pode sobrescrever esses valores em runtime via mensagem CONFIG
const DEFAULT_MIN_TEMP: f32 = 25.0;
const DEFAULT_MAX_TEMP: f32 = 25.0;
const DEFAULT_HYS_TEMP: f32 = 1.3;
const DEFAULT_MIN_HUM: f32 = 49.0;
const DEFAULT_MAX_HUM: f32 = 51.0;
const DEFAULT_HYS_HUM: f32 = 1.0;
const DEFAULT_MIN_CO2: f32 = 400.0;
const DEFAULT_HYS_CO2: f32 = 5.0;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum State {
    Heating,
    Cooling,
    Off,
}

pub struct Manager {
    max_temp: f32,
    min_temp: f32,
    min_hum: f32,
    max_hum: f32,
    min_co2: f32,
    hys_temp: f32,
    hys_hum: f32,
    hys_co2: f32,
    configured: bool,
    actuators: HashSet<u8>,
    sensors: HashSet<u8>,
    actuator_streams: HashMap<u8, Arc<Mutex<TcpStream>>>,
    sensor_last_seen: HashMap<u8, Instant>, // watchdog, rastreia quando cada sensor mandou dado pela última vez
    curr_temp: f32,
    curr_hum: f32,
    curr_co2: f32,
    temp_state: State,
    hum_state: bool,
    co2_state: bool,
}

impl Default for Manager {
    fn default() -> Manager {
        Self::new()
    }
}

impl Manager {
    pub fn new() -> Manager {
        Manager {
            // já começa com as configs padrão, então o gerenciador opera de cara
            max_temp: DEFAULT_MAX_TEMP,
            min_temp: DEFAULT_MIN_TEMP,
            min_hum: DEFAULT_MIN_HUM,
            max_hum: DEFAULT_MAX_HUM,
            min_co2: DEFAULT_MIN_CO2,
            hys_co2: DEFAULT_HYS_CO2,
            hys_temp: DEFAULT_HYS_TEMP,
            hys_hum: DEFAULT_HYS_HUM,
            configured: true,
            actuators: HashSet::new(),
            sensors: HashSet::new(),
            actuator_streams: HashMap::new(),
            sensor_last_seen: HashMap::new(),
            curr_temp: -1.0,
            curr_hum: -1.0,
            curr_co2: -1.0,
            temp_state: State::Off,
            hum_state: false,
            co2_state: false,
        }
    }

    pub fn run(self) {
        let manager_arc = Arc::new(Mutex::new(self));
        let listener = TcpListener::bind(PORT_SERVER).unwrap_or_else(|e| {
            eprintln!("gerenciador: erro ao abrir porta {}: {}", PORT_SERVER, e);
            eprintln!("dica: porta já em uso? tente: kill $(lsof -ti :{});", PORT_SERVER.split(':').last().unwrap_or("8080"));
            std::process::exit(1);
        });
        println!("Gerenciador ouvindo em {}", PORT_SERVER);

        // watchdog que desconecta sensor que ficou mais de 2s sem mandar dado
        let watchdog_manager = Arc::clone(&manager_arc);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(1500));
                let mut manager = watchdog_manager.lock().unwrap();
                let now = Instant::now();
                let dead: Vec<u8> = manager.sensor_last_seen
                    .iter()
                    .filter(|(_, t)| now.duration_since(**t) > Duration::from_secs(2))
                    .map(|(&id, _)| id)
                    .collect();
                for id in dead {
                    manager.sensors.remove(&id);
                    manager.sensor_last_seen.remove(&id);
                    println!("gerenciador: sensor {} desconectado por timeout (2 leituras perdidas)", id);
                }
            }
        });

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("Nova conexão: {}", stream.peer_addr().unwrap());
                    let manager_clone = Arc::clone(&manager_arc);
                    let stream_clone = Arc::new(Mutex::new(stream));
                    thread::spawn(move || {
                        handle_connection(stream_clone, manager_clone);
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
                // decai cada arquivo do registro que tem decaimento natural (temp não decai)
                for s in devices::SENSORS {
                    if s.decay > 0.0 {
                        apply_decay(s.file, s.decay);
                    }
                }
            }
        });
    }

    fn check_config(&self) -> bool {
        let values = vec![
            self.max_temp,
            self.min_temp,
            self.min_hum,
            self.max_hum,
            self.min_co2,
            self.hys_temp,
            self.hys_hum,
            self.hys_co2,
        ];
        !values.iter().any(|&v| v < 0.0)
    }

    fn temp_control(&self) -> State {
        match self.temp_state {
            State::Off => {
                if self.curr_temp > self.max_temp + self.hys_temp {
                    State::Cooling
                } else if self.curr_temp < self.min_temp - self.hys_temp {
                    State::Heating
                } else {
                    State::Off
                }
            }
            State::Cooling => {
                if self.curr_temp <= self.max_temp - self.hys_temp {
                    State::Off
                } else {
                    State::Cooling
                }
            }
            State::Heating => {
                if self.curr_temp >= self.min_temp + self.hys_temp {
                    State::Off
                } else {
                    State::Heating
                }
            }
        }
    }

    fn handle_connect(&mut self, payload: Connect, stream: Arc<Mutex<TcpStream>>) -> Option<Message> {
        let device_kind = payload.kind;
        let id = payload.id;

        // valida o id contra o registro central conforme o tipo declarado no connect
        match device_kind {
            0 => {
                if devices::sensor_by_id(id).is_none() {
                    println!("Sensor de id {} não está no registro; cadastro cancelado...", id);
                    return None;
                }
                self.sensors.insert(id);
                println!("Sensor cadastrado: {} ({})", id, devices::name_by_id(id));
            },
            1 => {
                if devices::actuator_by_id(id).is_none() {
                    println!("Atuador de id {} não está no registro; cadastro cancelado...", id);
                    return None;
                }
                self.actuators.insert(id);
                self.actuator_streams.insert(id, stream);
                println!("Atuador cadastrado: {} ({})", id, devices::name_by_id(id));
            },
            _ => {
                println!("Dispositivo desconhecido; cadastro cancelado...");
                return None;
            }
        };

        Some(Message::ack(MessageType::CONNECT))
    }

    fn send_act_cmd(&self, actuator_id: u8, command: u8) {
        if let Some(stream_arc) = self.actuator_streams.get(&actuator_id) {
            let mut stream = stream_arc.lock().unwrap();
            let msg = Message::new(MessageType::ActCmd, Payload::ActCmd(ActCmd { command }));
            if stream.write_all(&msg.encode()).is_err() {
                eprintln!("Gerenciador: Erro ao enviar comando para o atuador {}", actuator_id);
            }
        }
    }

    fn handle_sensor_data(&mut self, payload: SensorData) -> Option<Message> {
        let sensor_id = payload.sensor_id;
        let reading = payload.value;

        if self.sensors.get(&sensor_id).is_none() {
            println!("Sensor de id {} não conectado; informação descartada", sensor_id);
            return None;
        }

        // atualiza o timestamp pro watchdog saber que o sensor ainda tá vivo
        self.sensor_last_seen.insert(sensor_id, Instant::now());

        match sensor_id {
            0 => { // sensor de temperatura
                self.curr_temp = reading;
                let previous_state = self.temp_state;
                self.temp_state = self.temp_control();
                if previous_state != self.temp_state {
                    match self.temp_state {
                        State::Heating => {
                            self.send_act_cmd(3, 1); // liga aquecedor
                            self.send_act_cmd(4, 0); // desliga resfriador
                        },
                        State::Cooling => {
                            self.send_act_cmd(3, 0); // desliga aquecedor
                            self.send_act_cmd(4, 1); // liga resfriador
                        },
                        State::Off => {
                            self.send_act_cmd(3, 0); // desliga aquecedor
                            self.send_act_cmd(4, 0); // desliga resfriador
                        }
                    }
                }
            },
            1 => { // sensor de umidade
                self.curr_hum = reading;
                let previous_state = self.hum_state;
                // liga o irrigador abaixo de min_hum, desliga acima de max_hum, com histerese
                self.hum_state = onoff_control(self.curr_hum, self.min_hum, self.max_hum, self.hys_hum, self.hum_state);
                if previous_state != self.hum_state {
                    self.send_act_cmd(5, self.hum_state as u8); // liga/desliga irrigador
                }
            },
            2 => { // sensor de co2
                self.curr_co2 = reading;
                let previous_state = self.co2_state;
                // nota: co2 mantém uma banda em torno de min_co2 (passa min como inf e sup),
                // não tem max_co2 como a umidade tem, possível inconsistência pra revisar depois
                self.co2_state = onoff_control(self.curr_co2, self.min_co2, self.min_co2, self.hys_co2, self.co2_state);
                if previous_state != self.co2_state {
                    self.send_act_cmd(6, self.co2_state as u8); // liga/desliga injetor de co2
                }
            },
            _ => {}
        };

        Some(Message::ack(MessageType::SensorData))
    }

    fn handle_sensor_query(&mut self, payload: SensorQuery) -> Option<Message> {
        let query_id = payload.sensor_id;
        let sensor_value = match query_id {
            0 => self.curr_temp,
            1 => self.curr_hum,
            2 => self.curr_co2,
            _ => {
                println!("Recebido SENSOR_QUERY para um ID de sensor desconhecido: {}", query_id);
                return None;
            }
        };

        let res_payload = SensorRes { sensor_id: query_id, value: sensor_value };
        Some(Message::new(MessageType::SensorRes, Payload::SensorRes(res_payload)))
    }

    fn handle_config(&mut self, payload: Config) -> Option<Message> {
        let new_value = payload.value;
        let id = payload.key;
        let field_name = match id {
            0 => { self.min_temp = new_value; "min_temp" },
            1 => { self.max_temp = new_value; "max_temp" },
            2 => { self.hys_temp = new_value; "hys_temp" },
            3 => { self.min_hum = new_value; "min_hum" },
            4 => { self.max_hum = new_value; "max_hum" },
            5 => { self.hys_hum = new_value; "hys_hum" },
            6 => { self.min_co2 = new_value; "min_co2" },
            8 => { self.hys_co2 = new_value; "hys_co2" },
            _ => {
                println!("ID inválido: {}", id);
                return None;
            }
        };
        println!("Atualização do Gerenciador: {} -> {}", field_name, new_value);

        let prev: bool = self.configured;
        if !prev {
            self.configured = self.check_config();
            if self.configured {
                println!("Gerenciador 100% configurado e pronto p/uso!");
            }
        }

        Some(Message::ack(MessageType::CONFIG))
    }

    fn interpret_message(&mut self, message: Message, stream: Option<Arc<Mutex<TcpStream>>>) -> Option<Message> {
        // valida o magic
        if message.header.magic_number != u32::from_be_bytes(*b"PPPP") {
            println!("Protocolo não registrado; use PPPP");
            return None;
        }

        // connect e config sempre passam, o resto só depois que o servidor tiver configurado
        let blocked_kind = !matches!(
            message.header.kind,
            MessageType::CONFIG | MessageType::CONNECT
        );
        if blocked_kind && !self.configured {
            println!("Servidor ainda não configurado, mensagens fora de CONFIG/CONNECT serão ignoradas");
            return None;
        }

        if let Some(payload) = message.payload {
            match payload {
                Payload::Connect(p) => {
                    if let Some(stream) = stream {
                        return self.handle_connect(p, stream);
                    }
                    return None;
                },
                Payload::SensorData(p) => return self.handle_sensor_data(p),
                Payload::ActCmd(_) | Payload::SensorRes(_) => return None,
                Payload::SensorQuery(p) => return self.handle_sensor_query(p),
                Payload::Config(p) => return self.handle_config(p),
            }
        }

        // sem payload
        println!("Mensagem recebida sem payload ou não identificada;");
        None
    }
}

// controle liga/desliga genérico com histerese: liga abaixo de low-hys,
// desliga acima de high+hys, no meio mantém o estado atual (evita ligar/desligar à toa)
fn onoff_control(current: f32, low: f32, high: f32, hys: f32, state: bool) -> bool {
    if current < low - hys {
        true
    } else if current > high + hys {
        false
    } else {
        state
    }
}

// aplica decaimento num arquivo de ambiente, não deixa o valor ir abaixo de 0
fn apply_decay(file: &str, rate: f32) {
    match env_io::read_value(file) {
        Ok(mut value) => {
            value = (value - rate).max(0.0);
            if let Err(e) = env_io::write_value(file, value) {
                eprintln!("gerenciador: erro ao escrever decaimento em {}: {}", file, e);
            }
        }
        Err(e) => eprintln!("gerenciador: erro ao ler {} para decaimento: {}", file, e),
    }
}

fn handle_connection(stream_arc: Arc<Mutex<TcpStream>>, manager_arc: Arc<Mutex<Manager>>) {
    let mut receive_buffer: Vec<u8> = Vec::new();

    // clona o stream pra leitura sem lock: leitura e escrita ficam em handles separados,
    // então o send_act_cmd pode escrever no stream_arc enquanto esse thread bloqueia no read
    let mut read_stream = {
        let stream = stream_arc.lock().unwrap();
        stream.try_clone().expect("falha ao clonar stream para leitura")
    };

    loop {
        // fase 1: lê sem lock, só esse thread lê nessa conexão
        let chunk = {
            let mut tmp = [0u8; 1024];
            match read_stream.read(&mut tmp) {
                Ok(0) => { println!("Conexão fechada pelo peer."); return; }
                Ok(n) => tmp[..n].to_vec(),
                Err(_) => { println!("Conexão terminada."); return; }
            }
        };

        receive_buffer.extend_from_slice(&chunk);

        // fase 2: processa segurando só o lock do gerenciador
        // o send_act_cmd vai travar o stream_arc de algum atuador pra escrita,
        // mas como ninguém tá segurando esse lock no read, não tem deadlock
        let mut responses: Vec<Vec<u8>> = Vec::new();
        while let Some((message, consumed)) = Message::try_decode(&receive_buffer) {
            let mut manager = manager_arc.lock().unwrap();
            if let Some(response) = manager.interpret_message(message, Some(Arc::clone(&stream_arc))) {
                responses.push(response.encode());
            }
            receive_buffer.drain(..consumed);
        } // lock do gerenciador liberado aqui

        // fase 3: stream_arc é usado só pra escrita (acks, sensor_res), com lock curto
        if !responses.is_empty() {
            let mut stream = stream_arc.lock().unwrap();
            for bytes in responses {
                if stream.write_all(&bytes).is_err() {
                    eprintln!("Erro ao enviar resposta.");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // controle liga/desliga: liga abaixo do limite inferior menos a histerese
    #[test]
    fn onoff_turns_on_below_min() {
        // low=40, high=80, hys=5, estado atual desligado, valor 30 (< 40-5)
        assert!(onoff_control(30.0, 40.0, 80.0, 5.0, false));
    }

    // controle liga/desliga: desliga acima do limite superior mais a histerese
    #[test]
    fn onoff_turns_off_above_max() {
        // valor 90 (> 80+5), estado atual ligado
        assert!(!onoff_control(90.0, 40.0, 80.0, 5.0, true));
    }

    // dentro da banda morta o estado é mantido, evita liga/desliga nervoso
    #[test]
    fn onoff_keeps_state_in_band() {
        assert!(onoff_control(60.0, 40.0, 80.0, 5.0, true));   // estava ligado, continua
        assert!(!onoff_control(60.0, 40.0, 80.0, 5.0, false)); // estava desligado, continua
    }

    // monta um gerenciador de temperatura já configurado pra testar a histerese
    fn temp_manager(min: f32, max: f32, hys: f32, current: f32, state: State) -> Manager {
        let mut m = Manager::new();
        m.min_temp = min;
        m.max_temp = max;
        m.hys_temp = hys;
        m.curr_temp = current;
        m.temp_state = state;
        m
    }

    // desligado e quente demais: começa a resfriar
    #[test]
    fn temp_off_hot_cools() {
        let m = temp_manager(20.0, 30.0, 1.0, 32.0, State::Off);
        assert_eq!(m.temp_control(), State::Cooling);
    }

    // desligado e frio demais: começa a aquecer
    #[test]
    fn temp_off_cold_heats() {
        let m = temp_manager(20.0, 30.0, 1.0, 18.0, State::Off);
        assert_eq!(m.temp_control(), State::Heating);
    }

    // resfriando até cair abaixo de max-hys: desliga
    #[test]
    fn temp_cooling_turns_off_at_target() {
        let m = temp_manager(20.0, 30.0, 1.0, 28.0, State::Cooling); // 28 <= 30-1
        assert_eq!(m.temp_control(), State::Off);
    }

    // agora o gerenciador já nasce configurado com os valores padrão
    #[test]
    fn new_manager_starts_configured() {
        let m = Manager::new();
        assert!(m.configured);
        assert!(m.check_config());
    }

    // partindo de um gerenciador sem config, mandar as 8 configs marca ele como pronto
    #[test]
    fn configuring_all_sets_flag() {
        let mut m = Manager::new();
        m.configured = false; // simula gerenciador ainda sem config pra testar a transição
        let configs = [(0, 20.0), (1, 30.0), (2, 1.0), (3, 40.0), (4, 80.0), (5, 5.0), (6, 300.0), (8, 50.0)];
        for (k, v) in configs {
            assert!(m.handle_config(Config { key: k, value: v }).is_some());
        }
        assert!(m.configured);
        assert!(m.check_config());
    }

    // key de config inválida (7 não existe) é rejeitada
    #[test]
    fn invalid_config_key_rejected() {
        let mut m = Manager::new();
        assert!(m.handle_config(Config { key: 7, value: 1.0 }).is_none());
    }

    // se o gerenciador estiver marcado como não configurado, o portão bloqueia dados
    #[test]
    fn gate_blocks_data_when_unconfigured() {
        let mut m = Manager::new();
        m.configured = false;
        let msg = Message::new(MessageType::SensorData, Payload::SensorData(SensorData { sensor_id: 0, value: 25.0 }));
        assert!(m.interpret_message(msg, None).is_none());
    }

    // config sempre passa pelo portão mesmo sem configuração prévia
    #[test]
    fn gate_always_allows_config() {
        let mut m = Manager::new();
        let msg = Message::new(MessageType::CONFIG, Payload::Config(Config { key: 0, value: 20.0 }));
        assert!(m.interpret_message(msg, None).is_some());
    }
}
