# QuoteApp

QuoteApp is a simple Rust-based client-server application for streaming real-time stock quotes. The server generates random stock quotes for a predefined set of supported tickers and streams them to connected clients via UDP. The client connects to the server using TCP to register its interest and then receives quote updates on a specified UDP port.

## Features

- **Server**: Generates random stock price and volume data.
- **TCP Control**: Clients use TCP to connect and specify which tickers they want to monitor.
- **UDP Streaming**: Quotes are streamed over UDP for low-latency delivery.
- **Health Checks**: Implements a ping/pong mechanism to ensure clients are still active.

## Supported Tickers

The application supports a wide range of tickers, including:
`AAPL`, `MSFT`, `GOOGL`, `AMZN`, `NVDA`, `TSLA`, and many more.

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)

### Building the Project

To build the project, run:

```bash
cargo build --release
```

## Running the Application

### 1. Start the Server

The server requires a port number to listen for incoming TCP connections.

```bash
cargo run --bin server -- --port 8080
```

**Command line arguments:**
- `--port`: The TCP port the server will listen on (must be >= 1024).

### 2. Start the Client

The client needs the server's address, a local UDP port for receiving quotes, and a file containing the tickers it wants to subscribe to.

First, ensure you have a `tickers.txt` file (or any other text file) with one ticker per line:

```text
AAPL
MSFT
TSLA
```

Then run the client:

```bash
cargo run --bin client -- --server-addr 127.0.0.1:8080 --udp-port 9090 --tickers-file tickers.txt
```

**Command line arguments:**
- `--server-addr`: The address of the running server (IP:PORT).
- `--udp-port`: The local UDP port where the client will receive quotes.
- `--tickers-file`: Path to a file containing the list of tickers to subscribe to.

## Project Structure

- `src/bin/server.rs`: Entry point for the server application.
- `src/bin/client.rs`: Entry point for the client application.
- `src/server/`: Server logic and networking.
- `src/client/`: Client logic and networking.
- `src/quote.rs`: Shared quote generation and data structures.
