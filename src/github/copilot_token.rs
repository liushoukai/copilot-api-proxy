use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::api::{GITHUB_API_BASE_URL, editor_plugin_version, user_agent};

/// Copilot Token 响应
#[derive(Debug, Deserialize)]
pub struct CopilotTokenResponse {
    /// 短期 Copilot Token，用于调用 Copilot 补全 API
    pub token: String,
    /// Token 过期时间戳（Unix 秒）
    #[allow(dead_code)]
    pub expires_at: u64,
    /// 多少秒后刷新（通常约 1800 秒）
    pub refresh_in: u64,
}

/// 用 GitHub Access Token 换取 Copilot 专属短期 Token
pub async fn get_copilot_token(
    client: &reqwest::Client,
    github_token: &str,
    vscode_version: &str,
) -> Result<CopilotTokenResponse> {
    let url = format!("{}/copilot_internal/v2/token", GITHUB_API_BASE_URL);

    let resp = client
        .get(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("authorization", format!("token {}", github_token))
        .header("editor-version", format!("vscode/{}", vscode_version))
        .header("editor-plugin-version", editor_plugin_version())
        .header("user-agent", user_agent())
        .header("x-github-api-version", "2025-04-01")
        .header("x-vscode-user-agent-library-version", "electron-fetch")
        .send()
        .await
        .context("请求 Copilot Token 失败")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("获取 Copilot Token 失败：{}", text);
    }

    resp.json::<CopilotTokenResponse>()
        .await
        .context("解析 Copilot Token 响应失败")
}
