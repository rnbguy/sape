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

use clap::Parser;
pub use cli::*;
use color_eyre::eyre::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;
pub use types::*;

pub async fn run() -> Result<()> {
    color_eyre::install()?;
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    let namespace =
        std::env::var("SAPE_NAMESPACE").unwrap_or_else(|_| protocol::DEFAULT_NAMESPACE.to_string());

    if namespace.is_empty() {
        eprintln!("error: SAPE_NAMESPACE must not be empty");
        std::process::exit(1);
    }
    if namespace.contains('/') {
        eprintln!("error: SAPE_NAMESPACE must not contain '/'");
        std::process::exit(1);
    }
    if !namespace.is_ascii() || namespace.chars().any(|c| c.is_ascii_control()) {
        eprintln!("error: SAPE_NAMESPACE must contain only printable ASCII characters");
        std::process::exit(1);
    }

    let tunnel_proto = protocol::tunnel_protocol(&namespace);
    let jump_proto = protocol::jump_protocol(&namespace);
    info!(%namespace, "using protocol namespace");

    let command = cli::Cli::parse()
        .command
        .map_or_else(interactive::run_interactive, Ok)?;

    match command {
        cli::Command::Relay(opt) => {
            relay::run_relay(opt, tunnel_proto, jump_proto, &namespace).await
        },
        cli::Command::Listen(opt) => client::run_listen(opt, tunnel_proto, &namespace).await,
        cli::Command::Dial(opt) => {
            client::run_dial(opt, tunnel_proto, jump_proto, &namespace).await
        },
    }
}
