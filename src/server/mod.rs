mod error;

use crate::quote::{QuoteGenerator, StockQuote};
use crate::server::error::ServerError;
use crossbeam::channel::{Receiver, Sender, TryRecvError, TrySendError};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

static PING_MAX_TIMEOUT_SECS: u64 = 5;

pub struct Server {
    port: u16,
    running: Arc<AtomicBool>,
    clients: Vec<JoinHandle<Result<(), ServerError>>>,
    generator_thread: Option<JoinHandle<Result<(), ServerError>>>,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            running: Arc::new(AtomicBool::new(true)),
            clients: Vec::new(),
            generator_thread: None,
        }
    }

    pub fn run(&mut self) -> Result<(), ServerError> {
        let listener = TcpListener::bind(("0.0.0.0", self.port))?;
        listener.set_nonblocking(true)?;
        info!("listening on {}", listener.local_addr()?);

        self.register_ctrlc()?;

        let (sender, receiver) = crossbeam::channel::bounded::<StockQuote>(1);

        self.run_generator(sender)?;

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let peer_address = stream.peer_addr()?;
                    info!("accepted a new stream from {}", peer_address);
                    self.clients.push(thread::spawn({
                        let running = self.running.clone();
                        let quote_receiver = receiver.clone();
                        move || Self::handle_connection(stream, running, quote_receiver)
                    }));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    if !self.running.load(Ordering::SeqCst) {
                        break;
                    }
                }
                Err(e) => error!("connection failed: {}", e),
            }
        }

        if let Err(e) = self.generator_thread.take().unwrap().join() {
            error!("generator thread failed: {:?}", e);
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
    fn run_generator(&mut self, sender: Sender<StockQuote>) -> Result<(), ServerError> {
        let running = self.running.clone();

        self.generator_thread = Some(thread::spawn(move || {
            let mut generator = QuoteGenerator::new();

            // not optimal, but it enables interrupt thread
            'outer: while running.load(Ordering::SeqCst) {
                // Generate stock quotes and send them through the channel
                thread::sleep(Duration::from_millis(500));
                let ticker = QuoteGenerator::random_ticker();
                let quote = match generator.generate_quote(ticker) {
                    Ok(quote) => quote,
                    Err(e) => {
                        error!("failed to generate quote: {}", e);
                        continue;
                    }
                };
                info!("generated quote: {}", quote.to_string());
                while running.load(Ordering::SeqCst) {
                    let quote = quote.clone();
                    match sender.try_send(quote) {
                        Ok(_) => break,
                        Err(TrySendError::Disconnected(_)) => break 'outer,
                        Err(TrySendError::Full(_)) => {
                            thread::sleep(Duration::from_millis(500))
                        } // try again after a short delay,
                    }
                }
            }

            info!("generator thread terminated");

            Ok(())
        }));

        Ok(())
    }

    fn register_ctrlc(&self) -> Result<(), ctrlc::Error> {
        let r = self.running.clone();

        ctrlc::set_handler(move || {
            info!("received SIGINT");
            r.store(false, Ordering::SeqCst);
        })?;

        Ok(())
    }

    fn handle_connection(
        stream: TcpStream,
        running: Arc<AtomicBool>,
        quote_receiver: Receiver<StockQuote>,
    ) -> Result<(), ServerError> {
        stream.set_nonblocking(true)?;
        let mut writer = stream.try_clone()?;
        let mut reader = BufReader::new(stream);

        let _ = writer.write_all(b"Welcome to the Quote Server!\n");
        let _ = writer.flush();

        let mut streamers: Vec<JoinHandle<Result<(), ServerError>>> =
            Vec::new();

        let mut pingers: Vec<Arc<JoinHandle<Result<(), ServerError>>>> =
            Vec::new();

        let mut line = String::new();

        // flag to interrupt streaming by PING checking
        let streaming_flag = Arc::new(AtomicBool::new(true));

        // not optimal, but it enables interrupt thread
        while running.load(Ordering::SeqCst) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — client closed connection
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
                            let socket = Arc::new(UdpSocket::bind("127.0.0.1:0")?);
                            socket.set_nonblocking(true)?;

                            streaming_flag.store(true, Ordering::SeqCst);

                            if let (Some(address), Some(tickers)) = (address, tickers) {
                                let pinger_handle = Arc::new(thread::spawn({
                                    let running = running.clone();
                                    let address = address.to_string();
                                    let socket = socket.clone();
                                    let streaming_flag = streaming_flag.clone();
                                    move || {
                                        Self::handle_ping(socket, address, running, streaming_flag)
                                    }
                                }));

                                pingers.push(pinger_handle.clone());

                                streamers.push(thread::spawn({
                                    let running = running.clone();
                                    let address = address.to_string();
                                    let quote_receiver = quote_receiver.clone();
                                    let streaming_flag = streaming_flag.clone();
                                    let tickers =
                                        HashSet::from_iter(tickers.split(',').map(String::from));
                                    move || {
                                        Self::handle_streaming(
                                            socket,
                                            address,
                                            tickers,
                                            running,
                                            quote_receiver,
                                            streaming_flag,
                                            pinger_handle
                                        )
                                    }
                                }));
                                format!("OK: streaming to {} for {}\n", address, tickers)
                            } else {
                                "ERROR: usage STREAM host:port ticker1,ticker2\n".to_string()
                            }
                        }

                        Some("EXIT") => {
                            let _ = writer.write_all(b"BYE\n");
                            let _ = writer.flush();
                            break;
                        }

                        _ => "ERROR: unknown command\n".to_string(),
                    };

                    // sending response and again showing prompt
                    let _ = writer.write_all(response.as_bytes());
                    let _ = writer.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    // reading error - close connection
                    break;
                }
            }
        }

        for handle in streamers.drain(..) {
            if let Err(e) = handle.join() {
                error!("streamer thread failed: {:?}", e);
            }
        }

        for handle in pingers.drain(..) {
            if let Ok(handle) = Arc::try_unwrap(handle) {
                if let Err(e) = handle.join() {
                    error!("streamer thread failed: {:?}", e);
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

    fn handle_streaming(
        socket: Arc<UdpSocket>,
        address: String,
        tickers: HashSet<String>,
        running: Arc<AtomicBool>,
        quote_receiver: Receiver<StockQuote>,
        streaming_flag: Arc<AtomicBool>,
        pinger_handle: Arc<JoinHandle<Result<(), ServerError>>>,
    ) -> Result<(), ServerError> {
        info!("streaming started for {}", address);

        let mut first_quote_sent = false;

        while running.load(Ordering::SeqCst) && streaming_flag.load(Ordering::SeqCst) {
            match quote_receiver.try_recv() {
                Ok(quote) => {
                    info!("checking ticker of quote: {}", quote.to_string());
                    if !tickers.contains(quote.ticker.as_str()) {
                        continue;
                    }
                    let message = format!("QUOTE {}\n", quote.to_string());
                    if let Err(e) = socket.send_to(message.as_bytes(), &address) {
                        error!("failed to send quote to {}: {}", address, e);
                        break;
                    }
                    info!("quote {} sent to {}", quote.to_string(), address);
                    if !first_quote_sent {
                        first_quote_sent = true;
                        info!("!!!! first quote sent to {} -> unpark", address);
                        pinger_handle.thread().unpark();
                    }
                }
                Err(TryRecvError::Empty) => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }
        }

        if let Err(e) = socket.send_to(b"BYE\n", &address) {
            error!("failed to send BYE to {}: {}", address, e);
        }

        pinger_handle.thread().unpark();

        info!("sender stream ended for {}", address);

        Ok(())
    }

    fn handle_ping(
        socket: Arc<UdpSocket>,
        address: String,
        running: Arc<AtomicBool>,
        streaming_flag: Arc<AtomicBool>,
    ) -> Result<(), ServerError> {
        let mut buf = [0u8; 1024];

        info!("ping stream started for {}", address);

        thread::park();

        let mut start = Instant::now();

        info!("start checking ping for {}", address);

        // not optimal, but it enables interrupt thread
        while running.load(Ordering::SeqCst) {
            match socket.recv_from(&mut buf) {
                Ok((amount, src)) => {
                    let message = String::from_utf8_lossy(&buf[..amount]);
                    info!("received from {}: {}", src, message);
                    if message == "PING\n" {
                        start = Instant::now();
                        if let Err(e) = socket.send_to(b"PONG\n", &address) {
                            error!("failed to send pong: {:?}", e);
                        }
                    } else {
                        error!("received unexpected message: {}", message);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let elapsed = start.elapsed().as_secs();

                    if elapsed > PING_MAX_TIMEOUT_SECS {
                        warn!("ping timeout");
                        streaming_flag.store(false, Ordering::SeqCst);
                        break;
                    }

                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    // reading error - interrupt thread
                    error!("error receiving from {}: {}", address, e);
                    break;
                }
            }
        }

        info!("ping stream ended for {}", address);

        Ok(())
    }
}
