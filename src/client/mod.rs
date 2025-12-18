use crate::client::error::ClientError;
use log::info;
use socket2::{Domain, Protocol, Socket, Type};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::error;

mod error;

static PING_TIMEOUT: u64 = 2;

pub struct Client {
    address: SocketAddr,
    udp_port: u16,
    tickers_file: String,
    running: Arc<AtomicBool>,
}

impl Client {
    pub fn new(address: SocketAddr, udp_port: u16, tickers_file: String) -> Self {
        Self {
            address,
            udp_port,
            tickers_file,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn run(&self) -> Result<(), ClientError> {
        let (stream, mut reader) = self.connect(self.address)?;

        let command = self.create_command()?;

        self.register_ctrlc()?;

        let socket = Arc::new(UdpSocket::bind(format!("127.0.0.1:{}", self.udp_port))?);
        socket.set_nonblocking(true)?;

        let (tx, rx) = mpsc::channel::<SocketAddr>();

        let quotes_thread = std::thread::spawn({
            let socket = socket.clone();
            let running = self.running.clone();
            move || Self::handle_quotes(socket, tx, running)
        });

        info!("sending command: {}", command);

        self.send_command(&stream, &mut reader, &command)?;

        info!("command successfully sent");

        let ping_thread = std::thread::spawn({
            let socket = socket.clone();
            let running = self.running.clone();
            move || Self::handle_ping(socket, rx, running)
        });

        if let Err(e) = quotes_thread.join() {
            error!("error joining quotes thread: {:?}", e);
        }

        if let Err(e) = ping_thread.join() {
            error!("error joining ping thread: {:?}", e);
        }

        Ok(())
    }
}

impl Client {
    fn register_ctrlc(&self) -> Result<(), ctrlc::Error> {
        let r = self.running.clone();

        ctrlc::set_handler(move || {
            info!("received SIGINT");
            r.store(false, Ordering::SeqCst);
        })?;

        Ok(())
    }

    fn create_command(&self) -> Result<String, ClientError> {
        let tickers = self.load_tickers()?;

        Ok(format!("STREAM 127.0.0.1:{} {}", self.udp_port, tickers))
    }

    fn load_tickers(&self) -> Result<String, ClientError> {
        let tickers_file = File::open(&self.tickers_file)?;
        let reader = BufReader::new(tickers_file);

        let mut tickers: Vec<String> = Vec::new();

        for line in reader.lines() {
            tickers.push(line?);
        }

        Ok(tickers.join(","))
    }

    fn connect(&self, addr: SocketAddr) -> Result<(TcpStream, BufReader<TcpStream>), ClientError> {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;

        // Включаем TCP keepalive
        socket.set_keepalive(true)?;

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            socket.set_tcp_keepalive(
                &socket2::TcpKeepalive::new()
                    .with_time(Duration::from_secs(10))
                    .with_interval(Duration::from_secs(5)),
            )?;
        }

        socket.connect(&addr.into())?;
        let stream: TcpStream = socket.into();

        // тайм-аут на чтение
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;

        let mut reader = BufReader::new(stream.try_clone()?);

        // Читаем welcome message один раз
        let mut line = String::new();
        reader.read_line(&mut line)?;
        print!("{}", line);

        println!("Connected to server!");
        Ok((stream, reader))
    }

    fn send_command(
        &self,
        mut stream: &TcpStream,
        reader: &mut BufReader<TcpStream>,
        command: &str,
    ) -> io::Result<String> {
        stream.write_all(command.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        let mut buffer = String::new();
        let bytes = reader.read_line(&mut buffer)?;

        if bytes == 0 {
            // сервер закрыл соединение
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Server closed connection",
            ));
        }

        Ok(buffer)
    }

    fn handle_quotes(
        socket: Arc<UdpSocket>,
        sender: mpsc::Sender<SocketAddr>,
        running: Arc<AtomicBool>,
    ) -> Result<(), ClientError> {
        let mut buf = [0; 1024];

        info!("quotes thread started");

        let mut address_initialized = false;

        while running.load(Ordering::SeqCst) {
            match socket.recv_from(&mut buf) {
                Ok((amount, src)) => {
                    let message = String::from_utf8_lossy(&buf[..amount]);
                    info!("received from {}: {}", src, message);

                    if !address_initialized {
                        sender.send(src).unwrap();
                        address_initialized = true;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    // reading error - interrupt thread
                    error!("error receiving quote: {}", e);
                    break;
                }
            }
        }

        info!("quotes thread stopped");

        Ok(())
    }

    fn handle_ping(
        socket: Arc<UdpSocket>,
        receiver: mpsc::Receiver<SocketAddr>,
        running: Arc<AtomicBool>,
    ) -> Result<(), ClientError> {
        info!("ping thread started");

        let server_address = match receiver.recv() {
            Ok(addr) => addr,
            Err(_) => {
                info!("ping thread stopped");
                return Ok(());
            }
        };

        info!("address for ping acquired");

        while running.load(Ordering::SeqCst) {
            match socket.send_to(b"PING\n", server_address) {
                Err(e) => {
                    error!("error sending ping to {}: {}", server_address, e);
                    break;
                }
                Ok(_) => {
                    info!("sent ping to {}", server_address);
                    std::thread::sleep(Duration::from_secs(PING_TIMEOUT));
                }
            }
        }

        info!("ping thread stopped");

        Ok(())
    }
}
