use anyhow::Result;
use clap::Args;
use tracing::info;

use crate::config::paths::github_token_path;
use crate::state::AppState;
use crate::token::setup_github_token;

#[derive(Args)]
pub struct AuthArgs {
    /// Force re-authorization, ignoring the locally cached Token
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// Display the GitHub Token in the terminal after successful authorization
    #[arg(long, default_value_t = false)]
    pub show_token: bool,
}

pub async fn run(args: &AuthArgs) -> Result<()> {
    let state = AppState::new("1.117.0", None);

    setup_github_token(&state, args.force).await?;

    if args.show_token {
        if let Some(t) = state.github_token.read().await.as_deref() {
            info!("GitHub Token: {}", t);
        }
    }

    info!("GitHub Token written to: {}", github_token_path().display());
    Ok(())
}
