use clap::{Parser, Subcommand};

pub mod auth;
pub mod start;

/// copilot-api-proxy：GitHub Copilot API 代理工具
#[derive(Parser)]
#[command(name = "copilot-api-proxy", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 执行 GitHub Device Flow 授权，获取并缓存 Access Token
    Auth(auth::AuthArgs),

    /// 启动 Copilot API 代理服务
    Start(start::StartArgs),
}
