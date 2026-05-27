use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::api::{GITHUB_API_BASE_URL, editor_plugin_version, user_agent};

/// Copilot Token response
#[derive(Debug, Deserialize)]
pub struct CopilotTokenResponse {
    /// Short-lived Copilot Token for calling the Copilot completion API
    pub token: String,
    /// Token expiry Unix timestamp (seconds)
    #[allow(dead_code)]
    pub expires_at: u64,
    /// Seconds until the token should be refreshed (typically ~1800s)
    pub refresh_in: u64,
}

/// Exchange a GitHub Access Token for a short-lived Copilot-specific Token
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
        .context("failed to request Copilot Token")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to fetch Copilot Token: {}", text);
    }

    resp.json::<CopilotTokenResponse>()
        .await
        .context("failed to parse Copilot Token response")
}
