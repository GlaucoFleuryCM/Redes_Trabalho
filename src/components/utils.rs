use std::net::TcpStream;
use std::thread;
use std::time::Duration;

// tenta conectar em loop até dar certo, printando as tentativas de retry
pub fn connect(addr: &str, name: &str) -> TcpStream {
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => {
                println!("{}: conectado ao servidor.", name);
                return stream;
            }
            Err(_) => {
                eprintln!("{}: falha ao conectar, tentando novamente em 5s.", name);
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
}
