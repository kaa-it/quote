use anyhow::Result;
use std::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(clap::Parser, Debug)]
struct Args {
    #[arg(long)]
    port: u16,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info".parse::<EnvFilter>()?)
        .without_time()
        .with_target(false)
        .init();

    let args: Args = clap::Parser::parse();
    if args.port < 1024 {
        anyhow::bail!(
            "invalid port number: {}, must not be less then 1024",
            args.port
        );
    }

    let listener = TcpListener::bind(("0.0.0.0", args.port))?;
    info!("listening on {}", listener.local_addr()?);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                info!("accepted a new stream");
            }
            Err(e) => error!("connection failed: {}", e),
        }
    }

    Ok(())
}
