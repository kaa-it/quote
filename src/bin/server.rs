use anyhow::Result;
use quoteapp::server::Server;
use tracing::info;
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
    let mut server = Server::new(args.port);
    server.run()?;

    info!("server terminated");

    Ok(())
}
