use anyhow::Result;
use clap::Args;
use tracing::info;

use crate::copilot::models::get_models;
use crate::github::vscode_version::get_vscode_version;
use crate::server::serve;
use crate::state::AppState;
use crate::token::{setup_copilot_token, setup_github_token};

#[derive(Args)]
pub struct StartArgs {
    /// 监听端口
    #[arg(short, long, default_value_t = 4142)]
    pub port: u16,

    /// 开启详细日志（DEBUG 级别）
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// 直接提供 GitHub Token，跳过 Device Flow 授权
    #[arg(short = 'g', long)]
    pub github_token: Option<String>,

    /// 账户类型：individual / business / enterprise
    #[arg(short, long, default_value = "individual")]
    pub account_type: String,

    /// 授权成功后在终端显示 Token
    #[arg(long, default_value_t = false)]
    pub show_token: bool,
}

pub async fn run(args: &StartArgs) -> Result<()> {
    // 创建临时 Client 用于启动阶段（此时 state 还未建立）
    // reqwest 默认读取 HTTP_PROXY / HTTPS_PROXY 环境变量
    let bootstrap_client = reqwest::Client::new();

    // 动态获取最新 VSCode 版本，版本号影响 GitHub 返回的可用模型范围
    let vscode_version = get_vscode_version(&bootstrap_client).await;
    info!("VSCode 版本：{}", vscode_version);

    // 建立全局状态（内含共享 Client，同样自动读取代理环境变量）
    let state = AppState::new(&vscode_version);

    // 获取 GitHub Token
    if let Some(ref token) = args.github_token {
        info!("使用命令行提供的 GitHub Token");
        *state.github_token.write().await = Some(token.clone());
    } else {
        setup_github_token(&state, false).await?;
    }

    if args.show_token {
        if let Some(t) = state.github_token.read().await.as_deref() {
            info!("GitHub Token：{}", t);
        }
    }

    // 换取 Copilot Token 并启动后台自动刷新
    setup_copilot_token(state.clone()).await?;

    if args.show_token {
        if let Some(t) = state.copilot_token.read().await.as_deref() {
            info!("Copilot Token：{}", t);
        }
    }

    // 预热模型列表缓存
    match get_models(&state.client, &state).await {
        Ok(models) => {
            info!(
                "可用模型：\n{}",
                models
                    .data
                    .iter()
                    .map(|m| format!("  - {}", m.id))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            *state.models.write().await = Some(models);
        }
        Err(e) => {
            tracing::warn!("预热模型列表失败：{:#}", e);
        }
    }

    info!("账户类型：{}", args.account_type);
    serve(state, args.port).await
}
