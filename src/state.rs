use std::sync::Arc;
use tokio::sync::RwLock;

use crate::copilot::models::ModelsResponse;

/// 全局应用状态，使用 Arc + RwLock 支持多任务并发访问
#[derive(Clone)]
pub struct AppState {
    /// 共享 HTTP 客户端，支持显式代理配置
    pub client: reqwest::Client,
    /// GitHub Access Token（长期），从文件缓存或 Device Flow 获取
    pub github_token: Arc<RwLock<Option<String>>>,
    /// Copilot Token（短期，约30分钟），后台自动刷新
    pub copilot_token: Arc<RwLock<Option<String>>>,
    /// 启动时缓存的可用模型列表
    pub models: Arc<RwLock<Option<ModelsResponse>>>,
    /// 模拟的 VSCode 版本号，用于请求头
    pub vscode_version: Arc<RwLock<String>>,
}

impl AppState {
    /// 创建应用状态。proxy 为可选的代理地址（如 http://127.0.0.1:7890），
    /// 未传时自动读取 HTTP_PROXY / HTTPS_PROXY 环境变量。
    pub fn new(vscode_version: &str, proxy: Option<&str>) -> Self {
        let client = build_client(proxy).expect("构建 HTTP 客户端失败");
        Self {
            client,
            github_token: Arc::new(RwLock::new(None)),
            copilot_token: Arc::new(RwLock::new(None)),
            models: Arc::new(RwLock::new(None)),
            vscode_version: Arc::new(RwLock::new(vscode_version.to_string())),
        }
    }
}

/// 构建 reqwest Client。显式传入代理时优先使用，否则回退到环境变量。
fn build_client(proxy: Option<&str>) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::ClientBuilder::new();
    if let Some(url) = proxy {
        // 显式配置代理，同时覆盖 HTTP 和 HTTPS
        builder = builder.proxy(reqwest::Proxy::all(url)?);
    }
    // 未传 proxy 时 reqwest 默认仍会读取 HTTP_PROXY / HTTPS_PROXY 环境变量
    Ok(builder.build()?)
}
