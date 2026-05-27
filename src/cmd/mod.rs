use clap::{Parser, Subcommand};

pub mod auth;
pub mod start;

/// copilot-api-proxy: GitHub Copilot API proxy tool
#[derive(Parser)]
#[command(name = "copilot-api-proxy", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run GitHub Device Flow authorization and cache the resulting Access Token
    Auth(auth::AuthArgs),

    /// Start the Copilot API proxy server
    Start(start::StartArgs),
}
