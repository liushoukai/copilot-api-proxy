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

    /// HTTP/HTTPS 代理地址，例如 http://127.0.0.1:7890
    /// 等效于同时设置 HTTP_PROXY 和 HTTPS_PROXY 环境变量
    #[arg(long)]
    pub proxy: Option<String>,
}

pub async fn run(args: &StartArgs) -> Result<()> {
    // --proxy 参数写入环境变量，后续所有 reqwest::Client 均自动读取
    if let Some(ref proxy) = args.proxy {
        set_proxy_env(proxy);
        info!("代理已设置：{}", proxy);
    }

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

    // 预热模型列表缓存，失败时指数退避重试，全部失败则终止启动
    const MAX_RETRIES: u32 = 3;
    let models = fetch_models_with_retry(&state, MAX_RETRIES).await?;

    info!(
        "可用模型：\n{}",
        models
            .data
            .iter()
            .map(|m| format!("  - {}", m.id))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 检查是否包含 claude 模型，若无则提前警告
    // GitHub Copilot 会按请求来源 IP 返回不同的模型列表：
    // 中国大陆 IP 直连时，服务端会过滤掉 claude-* 模型（Anthropic 地区限制）；
    // 走海外代理后，返回完整列表，claude-* 才会出现。
    let claude_count = models.data.iter().filter(|m| m.id.starts_with("claude-")).count();
    if claude_count == 0 {
        tracing::warn!(
            "⚠️  模型列表中没有 claude-* 模型，Claude Code / Anthropic 请求将无法正常工作。\
             这通常是因为当前出口 IP 位于中国大陆，Copilot 服务端会按地区过滤 Claude 模型。\
             请通过 --proxy http://127.0.0.1:7890 参数或 HTTP_PROXY/HTTPS_PROXY 环境变量设置海外代理后重新启动。"
        );
    }
    *state.models.write().await = Some(models);

    info!("账户类型：{}", args.account_type);
    serve(state, args.port).await
}

/// 将代理地址写入 HTTP_PROXY / HTTPS_PROXY 环境变量
fn set_proxy_env(proxy: &str) {
    // SAFETY: 调用方（run）在单线程启动阶段执行，尚未创建任何子线程
    unsafe {
        std::env::set_var("HTTP_PROXY", proxy);
        std::env::set_var("HTTPS_PROXY", proxy);
    }
}

/// 获取模型列表，失败时按指数退避重试，全部失败则返回错误
async fn fetch_models_with_retry(state: &AppState, max_retries: u32) -> Result<crate::copilot::models::ModelsResponse> {
    let mut last_err = anyhow::anyhow!("未知错误");
    for attempt in 0..max_retries {
        if attempt > 0 {
            // 指数退避：1s、2s、4s …
            let delay = std::time::Duration::from_secs(1 << (attempt - 1));
            tracing::warn!("获取模型列表失败，{} 秒后重试（第 {}/{} 次）…", delay.as_secs(), attempt, max_retries - 1);
            tokio::time::sleep(delay).await;
        }
        match get_models(&state.client, state).await {
            Ok(models) => return Ok(models),
            Err(e) => last_err = e,
        }
    }
    tracing::error!(
        "⚠️  获取模型列表连续失败 {} 次，终止启动。\
         请检查网络是否可达 api.githubcopilot.com，或通过 HTTP_PROXY/HTTPS_PROXY 设置代理。",
        max_retries
    );
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_env_is_set() {
        let proxy = "http://127.0.0.1:17890";
        set_proxy_env(proxy);
        assert_eq!(std::env::var("HTTP_PROXY").unwrap(), proxy);
        assert_eq!(std::env::var("HTTPS_PROXY").unwrap(), proxy);
    }

    #[test]
    fn test_proxy_arg_is_optional() {
        // 不传 --proxy 时，已有的环境变量不应被覆盖
        unsafe {
            std::env::set_var("HTTP_PROXY", "http://original:8888");
        }
        // 模拟 proxy 为 None，不调用 set_proxy_env
        let proxy: Option<String> = None;
        if let Some(ref p) = proxy {
            set_proxy_env(p);
        }
        assert_eq!(std::env::var("HTTP_PROXY").unwrap(), "http://original:8888");
    }
}
