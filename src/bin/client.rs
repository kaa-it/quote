use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info".parse::<EnvFilter>()?)
        .without_time()
        .with_target(false)
        .init();

    Ok(())
}
