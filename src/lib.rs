pub mod cli;
pub mod client;
pub mod forward;
pub mod interactive;
pub mod jump;
pub mod netcat;
pub mod pairing;
pub mod protocol;
pub mod relay;
pub mod tunnel;
pub mod types;

pub use cli::*;
pub use types::*;


use clap::Parser;
use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

pub async fn run() -> Result<()> {
    color_eyre::install()?;
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(env_filter).try_init();

    let command = cli::Cli::parse().command.map_or_else(interactive::run_interactive, Ok)?;

    match command {
        cli::Command::Relay(opt) => relay::run_relay(opt).await,
        cli::Command::Listen(opt) => client::run_listen(opt).await,
        cli::Command::Dial(opt) => client::run_dial(opt).await,
    }
}