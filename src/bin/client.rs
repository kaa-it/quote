use std::net::SocketAddr;
use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;
use quoteapp::client::Client;

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(long)]
    server_addr: String,
    #[clap(long)]
    udp_port: u16,
    #[clap(long)]
    tickers_file: String,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info".parse::<EnvFilter>()?)
        //.without_time()
        .with_target(false)
        .init();

    let args: Args = clap::Parser::parse();

    info!("server addr: {}", args.server_addr);

    let address: SocketAddr = args.server_addr.parse()?;

    let client = Client::new(address, args.udp_port, args.tickers_file);

    client.run()?;

    Ok(())
}
