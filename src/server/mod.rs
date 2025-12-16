mod error;

use crate::server::error::ServerError;
use rand::Rng;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{error, info};

pub struct Server {
    port: u16,
    running: Arc<AtomicBool>,
    clients: Vec<JoinHandle<Result<(), ServerError>>>,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            running: Arc::new(AtomicBool::new(true)),
            clients: Vec::new(),
        }
    }

    pub fn run(&mut self) -> Result<(), ServerError> {
        let listener = TcpListener::bind(("0.0.0.0", self.port))?;
        listener.set_nonblocking(true)?;
        info!("listening on {}", listener.local_addr()?);

        self.register_ctrlc()?;

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    info!("accepted a new stream");
                    self.clients.push(thread::spawn({
                        let running = self.running.clone();
                        move || Self::handle_connection(stream, running)
                    }));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !self.running.load(Ordering::SeqCst) {
                        break;
                    }
                }
                Err(e) => error!("connection failed: {}", e),
            }
        }

        for handle in self.clients.drain(..) {
            if let Err(e) = handle.join() {
                error!("client thread failed: {:?}", e);
            }
        }

        Ok(())
    }
}

impl Server {
    fn register_ctrlc(&self) -> Result<(), ctrlc::Error> {
        let r = self.running.clone();

        ctrlc::set_handler(move || {
            info!("received SIGINT");
            r.store(false, Ordering::SeqCst);
        })?;

        Ok(())
    }

    fn handle_connection(stream: TcpStream, running: Arc<AtomicBool>) -> Result<(), ServerError> {
        stream.set_nonblocking(true)?;
        let mut writer = stream.try_clone()?;
        let mut reader = BufReader::new(stream);

        let _ = writer.write_all(b"Welcome to the Quote Server!\n");
        let _ = writer.flush();

        let mut line = String::new();
        while running.load(Ordering::SeqCst) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — клиент закрыл соединение
                    break;
                }
                Ok(_) => {
                    let input = line.trim();
                    if input.is_empty() {
                        let _ = writer.flush();
                        continue;
                    }

                    let mut parts = input.split_whitespace();
                    let response = match parts.next() {
                        Some("STREAM") => {
                            // let id = parts.next().and_then(|s| s.parse::<u32>().ok());
                            // let name = parts.next();
                            // let size = parts.next().and_then(|s| s.parse::<u32>().ok());
                            //
                            // if let (Some(id), Some(name), Some(size)) = (id, name, size) {
                            //     let item = Item {
                            //         name: name.to_string(),
                            //         size,
                            //     };
                            //     let mut v = vault.lock().unwrap();
                            //     match v.put(id, item, 100) {
                            //         Ok(_) => "OK: item stored\n".to_string(),
                            //         Err(VaultError::VaultFull) => "ERROR: vault full\n".to_string(),
                            //         Err(VaultError::CellFull) => "ERROR: cell full\n".to_string(),
                            //         _ => "ERROR: unknown\n".to_string(),
                            //     }
                            // } else {
                            //     "ERROR: usage PUT <id> <name> <size>\n".to_string()
                            // }
                            "STREAM: command received!\n".to_string()
                        }

                        Some("EXIT") => {
                            let _ = writer.write_all(b"BYE\n");
                            let _ = writer.flush();
                            break;
                        }

                        Some("PING") => {
                            let mut rng = rand::rng();

                            // Случайная задержка от 1 до 5 секунд
                            let delay_secs = rng.random_range(1..=5);
                            std::thread::sleep(Duration::from_secs(delay_secs));
                            "PONG\n".to_string()
                        }

                        _ => "ERROR: unknown command\n".to_string(),
                    };

                    // отправляем ответ и снова показываем prompt
                    let _ = writer.write_all(response.as_bytes());
                    let _ = writer.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(_) => {
                    // ошибка чтения — закрываем
                    break;
                }
            }
        }

        if !running.load(Ordering::SeqCst) {
            let _ = writer.write_all(b"BYE\n");
            let _ = writer.flush();
        }

        info!("connection terminated");

        Ok(())
    }
}
