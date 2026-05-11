use anyhow::Result;
use clap::Args;
use tracing::info;

use crate::config::paths::github_token_path;
use crate::state::AppState;
use crate::token::setup_github_token;

#[derive(Args)]
pub struct AuthArgs {
    /// 强制重新授权，忽略本地缓存的 Token
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// 授权成功后在终端显示 GitHub Token
    #[arg(long, default_value_t = false)]
    pub show_token: bool,
}

pub async fn run(args: &AuthArgs) -> Result<()> {
    let state = AppState::new("1.117.0", None);

    setup_github_token(&state, args.force).await?;

    if args.show_token {
        if let Some(t) = state.github_token.read().await.as_deref() {
            info!("GitHub Token：{}", t);
        }
    }

    info!("GitHub Token 已写入：{}", github_token_path().display());
    Ok(())
}
