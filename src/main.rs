mod anthropic;
mod cmd;
mod config;
mod copilot;
mod github;
mod server;
mod state;
mod token;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use cmd::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // --verbose takes priority; otherwise read LOG_LEVEL env var; default to INFO
    let filter = match &cli.command {
        Commands::Start(args) if args.verbose => EnvFilter::new("debug"),
        _ => {
            let level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
            EnvFilter::new(level)
        }
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match &cli.command {
        Commands::Auth(args) => cmd::auth::run(args).await,
        Commands::Start(args) => cmd::start::run(args).await,
    }
}
