use std::sync::Arc;
use tokio::sync::RwLock;

use crate::copilot::models::ModelsResponse;

/// 全局应用状态，使用 Arc + RwLock 支持多任务并发访问
#[derive(Clone)]
pub struct AppState {
    /// 共享 HTTP 客户端，自动读取 HTTP_PROXY / HTTPS_PROXY 环境变量
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
    pub fn new(vscode_version: &str) -> Self {
        Self {
            // reqwest::Client::new() 默认读取 HTTP_PROXY / HTTPS_PROXY 环境变量
            client: reqwest::Client::new(),
            github_token: Arc::new(RwLock::new(None)),
            copilot_token: Arc::new(RwLock::new(None)),
            models: Arc::new(RwLock::new(None)),
            vscode_version: Arc::new(RwLock::new(vscode_version.to_string())),
        }
    }
}
