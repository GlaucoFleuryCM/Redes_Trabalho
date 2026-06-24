use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::thread;
use std::time::Duration;

const LOCK_RETRY_MS: u64 = 10;

fn get_lock_path(data_path: &str) -> String {
    data_path.replace(".txt", ".lock")
}

// Tenta adquirir o lock. Retorna true se bem sucedido, false caso contrário.
fn acquire_lock_internal(lock_path: &str) -> bool {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
        .is_ok()
}

// Libera o lock
fn release_lock_internal(lock_path: &str) {
    let _ = fs::remove_file(lock_path);
}

pub fn read_value(path: &str) -> Result<f32, std::io::Error> {
    let lock_path = get_lock_path(path);
    
    // Tenta adquirir o lock em loop
    while !acquire_lock_internal(&lock_path) {
        thread::sleep(Duration::from_millis(LOCK_RETRY_MS));
    }

    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    release_lock_internal(&lock_path);

    contents
        .trim()
        .parse::<f32>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub fn write_value(path: &str, value: f32) -> Result<(), std::io::Error> {
    let lock_path = get_lock_path(path);

    // Tenta adquirir o lock em loop
    while !acquire_lock_internal(&lock_path) {
        thread::sleep(Duration::from_millis(LOCK_RETRY_MS));
    }

    let mut file = File::create(path)?;
    file.write_all(value.to_string().as_bytes())?;

    release_lock_internal(&lock_path);

    Ok(())
}

pub fn init_env_file(path: &str, default_value: f32) {
    if let Err(e) = write_value(path, default_value) {
        eprintln!("Falha ao inicializar o arquivo de ambiente {}: {}", path, e);
    }
    // Garante que o lock não exista no início
    let lock_path = get_lock_path(path);
    if fs::metadata(&lock_path).is_ok() {
        let _ = fs::remove_file(&lock_path);
    }
}
