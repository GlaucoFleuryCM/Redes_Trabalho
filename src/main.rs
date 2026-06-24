use clap::Parser;

mod protocolo;
mod componentes;

use componentes::{
    atuador::{Atuador, AtuadorTipo},
    cliente::Cliente,
    gerenciador::Gerenciador,
    sensor::Sensor,
    env_io,
};

const TEMP_FILE: &str = "src/env_vars/temp.txt";
const HUM_FILE: &str = "src/env_vars/hum.txt";
const CO2_FILE: &str = "src/env_vars/co2.txt";

#[derive(Parser)]
#[command(name = "estufa")]
#[command(about = "Simulador de estufa", long_about = None)]
struct Cli {
    #[command(subcommand)]
    componente: Componentes,
}

#[derive(Parser, Clone)]
enum Componentes {
    /// Roda o gerenciador
    Gerenciador,
    /// Roda um sensor
    Sensor {
        /// ID do sensor
        #[arg(short, long)]
        id: u8,
    },
    /// Roda um atuador
    Atuador {
        /// ID do atuador
        #[arg(short, long)]
        id: u8,
    },
    /// Roda o cliente
    Cliente,
    /// Roda a simulação completa
    Completo,
}


fn main() {
    let cli = Cli::parse();

    match cli.componente {
        Componentes::Gerenciador => {
            println!("=== Iniciando Gerenciador ===");
            env_io::init_env_file(TEMP_FILE, 25.0);
            env_io::init_env_file(HUM_FILE, 50.0);
            env_io::init_env_file(CO2_FILE, 400.0);
            let gerenciador = Gerenciador::new();
            gerenciador.start_decay_thread();
            gerenciador.run();
        },
        Componentes::Sensor { id } => {
            println!("=== Iniciando Sensor ID: {} ===", id);
            let sensor = Sensor::new(id);
            sensor.start();
            // Keep the main thread alive
            std::thread::park();
        },
        Componentes::Atuador { id } => {
            println!("=== Iniciando Atuador ID: {} ===", id);
            let tipo = match id {
                3 => AtuadorTipo::Aquecedor,
                4 => AtuadorTipo::Resfriador,
                5 => AtuadorTipo::Irrigador,
                6 => AtuadorTipo::InjetorCO2,
                _ => {
                    eprintln!("ID de atuador inválido: {}", id);
                    return;
                }
            };
            let atuador = Atuador::new(id, tipo);
            atuador.start();
            // Keep the main thread alive
            std::thread::park();
        },
        Componentes::Cliente => {
            println!("=== Iniciando Cliente ===");
            let cliente = Cliente::new();
            cliente.run();
        },
        Componentes::Completo => {
            run_completo();
        },
    }
}

fn run_completo(){
    use std::thread;
    use std::time::Duration;
    println!("=== Iniciando Simulação da Estufa ===");

    // 1. Inicializa os arquivos de ambiente
    println!("Inicializando arquivos de ambiente...");
    env_io::init_env_file(TEMP_FILE, 25.0);
    env_io::init_env_file(HUM_FILE, 50.0);
    env_io::init_env_file(CO2_FILE, 400.0);
    println!("Arquivos de ambiente prontos.");

    // 2. Cria os componentes
    let gerenciador = Gerenciador::new();

    // Sensores
    let sensor_temp = Sensor::new(0); // ID 0 para Temperatura
    let sensor_hum = Sensor::new(1);  // ID 1 para Umidade
    let sensor_co2 = Sensor::new(2);  // ID 2 para CO2

    // Atuadores
    let atuador_aq = Atuador::new(3, AtuadorTipo::Aquecedor);      // ID 3
    let atuador_res = Atuador::new(4, AtuadorTipo::Resfriador);     // ID 4
    let atuador_irr = Atuador::new(5, AtuadorTipo::Irrigador);    // ID 5
    let atuador_co2 = Atuador::new(6, AtuadorTipo::InjetorCO2);   // ID 6

    let cliente = Cliente::new();

    // 3. Inicia as threads dos componentes
    println!("Iniciando componentes em threads separadas...");

    // Thread do Gerenciador (decaimento do ambiente)
    gerenciador.start_decay_thread();

    // Inicia a thread principal do Gerenciador
    let manager_handle = thread::spawn(move || {
        gerenciador.run();
    });

    // Dá um tempo para o servidor do gerenciador subir antes de iniciar os clientes
    thread::sleep(Duration::from_secs(2));

    // Threads dos Sensores
    sensor_temp.start();
    sensor_hum.start();
    sensor_co2.start();

    // Threads dos Atuadores
    atuador_aq.start();
    atuador_res.start();
    atuador_irr.start();
    atuador_co2.start();

    // Dá um tempo para os dispositivos conectarem antes do cliente
    thread::sleep(Duration::from_secs(1));

    // Thread do Cliente
    let client_handle = thread::spawn(move || {
        cliente.run();
    });

    // Mantém a thread principal viva esperando a conclusão das outras threads.
    manager_handle.join().unwrap();
    client_handle.join().unwrap();

    println!("=== Simulação da Estufa Terminada ===");
}
