use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use file_locking::FileExt;

// sinaliza que o programa tá encerrando (ctrl+c); enquanto true, read/write não
// mexem mais nos arquivos, senão alguma thread recriaria o arquivo logo depois da limpeza
pub static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

fn shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::Relaxed)
}

fn lock_path(data_path: &str) -> String {
    data_path.replace(".txt", ".lock")
}

// abre (ou cria) o arquivo de lock associado ao arquivo de dados
fn open_lock(lock_path: &str) -> Result<File, std::io::Error> {
    OpenOptions::new().write(true).create(true).open(lock_path)
}

fn lock_err(e: file_locking::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}

pub fn read_value(path: &str) -> Result<f32, std::io::Error> {
    if shutting_down() {
        return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "encerrando"));
    }
    let lf = open_lock(&lock_path(path))?;
    let _guard = lf.lock_exclusive().map_err(lock_err)?;

    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    contents
        .trim()
        .parse::<f32>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn write_value(path: &str, value: f32) -> Result<(), std::io::Error> {
    if shutting_down() {
        return Ok(());
    }
    let lf = open_lock(&lock_path(path))?;
    let _guard = lf.lock_exclusive().map_err(lock_err)?;

    let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
    file.write_all(value.to_string().as_bytes())
}

pub fn init_env_file(path: &str, default_value: f32) {
    if let Err(e) = write_value(path, default_value) {
        eprintln!("falha ao inicializar o arquivo de ambiente {}: {}", path, e);
    }
}

// apaga o arquivo de ambiente e o lock dele, usado na limpeza ao sair (ctrl+c)
pub fn remove_env_file(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(lock_path(path));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // monta um caminho .txt único no dir temporário do sistema pra não colidir entre testes
    fn temp_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("estufa_test_{}.txt", name));
        p.to_str().unwrap().to_string()
    }

    // escrever e depois ler tem que devolver o mesmo valor
    #[test]
    fn write_read_round_trip() {
        let path = temp_path("round_trip");
        let _ = fs::remove_file(&path);
        write_value(&path, 23.5).unwrap();
        assert_eq!(read_value(&path).unwrap(), 23.5);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(lock_path(&path));
    }

    // init cria o arquivo com o valor padrão mesmo se ele não existe antes
    #[test]
    fn init_creates_file() {
        let path = temp_path("init");
        let _ = fs::remove_file(&path);
        init_env_file(&path, 99.0);
        assert_eq!(read_value(&path).unwrap(), 99.0);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(lock_path(&path));
    }

    // escrita sobrescreve o valor anterior em vez de acumular
    #[test]
    fn write_overwrites() {
        let path = temp_path("overwrite");
        let _ = fs::remove_file(&path);
        write_value(&path, 1.0).unwrap();
        write_value(&path, 2.0).unwrap();
        assert_eq!(read_value(&path).unwrap(), 2.0);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(lock_path(&path));
    }

    // o caminho do lock é o mesmo do dado mas com extensão .lock
    #[test]
    fn lock_path_swaps_extension() {
        assert_eq!(lock_path("src/env_vars/temp.txt"), "src/env_vars/temp.lock");
    }
}
