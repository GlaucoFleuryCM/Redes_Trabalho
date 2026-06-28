use clap::Parser;
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};

mod protocol;
mod components;

use components::{
    actuator::Actuator,
    client::Client,
    manager::Manager,
    sensor::Sensor,
    devices,
    env_io,
};

// lista global dos processos filhos do modo completo, pro handler de ctrl+c poder matá-los
static CHILDREN: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
fn children() -> &'static Mutex<Vec<Child>> {
    CHILDREN.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Parser)]
#[command(name = "estufa")]
struct Cli {
    #[command(subcommand)]
    component: Component,
}

#[derive(Parser, Clone)]
enum Component {
    #[command(name = "gerenciador")]
    Manager,
    #[command(name = "sensor")]
    Sensor {
        #[arg(short, long)]
        id: u8,
    },
    #[command(name = "atuador")]
    Actuator {
        #[arg(short, long)]
        id: u8,
    },
    #[command(name = "cliente")]
    Client,
    #[command(name = "completo")]
    Complete,
}


fn main() {
    let cli = Cli::parse();

    /* remover arquivos do ambiente quando interromper o progama com ctrl-c */
    ctrlc::set_handler(|| {
        println!("\nInterrompido, limpando os arquivos de ambiente...");
        // mata os processos filhos (modo completo) pra ninguém recriar arquivo depois da limpeza
        if let Some(lock) = CHILDREN.get() {
            if let Ok(mut kids) = lock.lock() {
                for child in kids.iter_mut() {
                    let _ = child.kill();
                }
            }
        }
        env_io::SHUTTING_DOWN.store(true, std::sync::atomic::Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(100));
        for s in devices::SENSORS {
            env_io::remove_env_file(s.file);
        }
        std::process::exit(0);
    }).expect("erro ao registrar handler de ctrl+c");

    match cli.component {
        Component::Manager => {
            println!("Iniciando Gerenciador");
            for s in devices::SENSORS {
                env_io::init_env_file(s.file, s.initial_value);
            }
            let manager = Manager::new();
            manager.start_decay_thread();
            manager.run();
        },
        Component::Sensor { id } => {
            println!("Iniciando Sensor {}", id);
            let sensor = Sensor::new(id);
            sensor.start();
            // mantém a thread principal rodando
            std::thread::park();
        },
        Component::Actuator { id } => {
            if devices::actuator_by_id(id).is_none() {
                return;
            }
            let actuator = Actuator::new(id);
            actuator.start();
            // mantém a thread principal rodando
            std::thread::park();
        },
        Component::Client => {
            println!("Iniciando Cliente");
            let client = Client::new();
            client.run();
        },
        Component::Complete => {
            run_complete();
        },
    }
}

fn run_complete() {
    use std::thread;
    use std::time::Duration;
    println!("Iniciando Simulação da Estufa (processos separados)");

    // sobe cada componente como um processo separado, reinvocando esse mesmo binário
    // com o subcomando dele; os filhos herdam o stdout/stderr, então os logs aparecem
    // todos no mesmo terminal
    let exe = std::env::current_exe().expect("não achou o próprio executável");
    let spawn = |args: &[&str]| {
        let child = Command::new(&exe).args(args).spawn().expect("falha ao subir processo filho");
        children().lock().unwrap().push(child);
    };

    // o gerenciador sobe primeiro, inicializa os arquivos de ambiente e abre a porta
    spawn(&["gerenciador"]);
    thread::sleep(Duration::from_secs(2));

    // um processo por sensor e por atuador, tudo vindo do registro central
    for s in devices::SENSORS {
        let id = s.id.to_string();
        spawn(&["sensor", "--id", &id]);
    }
    for a in devices::ACTUATORS {
        let id = a.id.to_string();
        spawn(&["atuador", "--id", &id]);
    }

    // dá um tempo pros dispositivos conectarem antes do cliente
    thread::sleep(Duration::from_secs(1));
    spawn(&["cliente"]);

    // segura o processo pai vivo até o ctrl+c (os filhos rodam sozinhos)
    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
