mod error;

use crate::server::error::ServerError;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::{error, info, warn};

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

        let mut streamers: Vec<JoinHandle<Result<(), crate::server::error::ServerError>>> =
            Vec::new();

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
                            let address = parts.next();
                            let tickers = parts.next();
                            let socket = Arc::new(UdpSocket::bind("localhost:0")?);
                            socket.set_nonblocking(true)?;
                            info!("{}", socket.local_addr()?);

                            if let (Some(address), Some(tickers)) = (address, tickers) {
                                streamers.push(thread::spawn({
                                    let running = running.clone();
                                    let address = address.to_string();
                                    let socket = socket.clone();
                                    let tickers =
                                        HashSet::from_iter(tickers.split(',').map(String::from));
                                    move || {
                                        Self::handle_streaming(socket, address, tickers, running)
                                    }
                                }));
                                streamers.push(thread::spawn({
                                    let running = running.clone();
                                    let address = address.to_string();
                                    move || Self::handle_ping(socket, address, running)
                                }));
                                format!("OK: streaming to {} for {}", address, tickers)
                            } else {
                                "ERROR: usage STREAM host:port ticker1,ticker2".to_string()
                            }
                        }

                        Some("EXIT") => {
                            let _ = writer.write_all(b"BYE\n");
                            let _ = writer.flush();
                            break;
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

        for handle in streamers.drain(..) {
            if let Err(e) = handle.join() {
                error!("streamer thread failed: {:?}", e);
            }
        }

        if !running.load(Ordering::SeqCst) {
            let _ = writer.write_all(b"BYE\n");
            let _ = writer.flush();
        }

        info!("connection terminated");

        Ok(())
    }

    fn handle_streaming(
        socket: Arc<UdpSocket>,
        address: String,
        tickers: HashSet<String>,
        running: Arc<AtomicBool>,
    ) -> Result<(), ServerError> {
        while running.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_secs(1));
        }

        info!("stream ended");

        Ok(())
    }

    fn handle_ping(
        socket: Arc<UdpSocket>,
        address: String,
        running: Arc<AtomicBool>,
    ) -> Result<(), ServerError> {
        let mut buf = [0u8; 1024];

        info!("ping stream started for {}", address);

        while running.load(Ordering::SeqCst) {
            match socket.recv_from(&mut buf) {
                Ok((amount, src)) => {
                    let message = String::from_utf8_lossy(&buf[..amount]);
                    info!("received from {}: {}", src, message);
                    if message == "PING\n" {
                        if let Err(e) = socket.send_to(b"PONG\n", &address) {
                            warn!("failed to send pong: {:?}", e);
                        }
                    } else {
                        warn!("received unexpected message: {}", message);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    // ошибка чтения — закрываем
                    warn!("error receiving from {}: {}", address, e);
                    break;
                }
            }
        }

        info!("ping stream ended for {}", address);

        Ok(())
    }
}
