use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::api::{editor_plugin_version, user_agent};
use crate::state::AppState;

// ── 请求 / 响应类型 ─────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct EmbeddingRequest {
    pub input: Value,
    pub model: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<Embedding>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Embedding {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}

/// 向 Copilot API 发起 embeddings 请求
pub async fn create_embeddings(
    client: &reqwest::Client,
    state: &AppState,
    payload: EmbeddingRequest,
) -> Result<EmbeddingResponse> {
    let copilot_token = state
        .copilot_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Copilot Token 未设置"))?;

    let vscode_version = state.vscode_version.read().await.clone();

    let resp = client
        .post("https://api.githubcopilot.com/embeddings")
        .bearer_auth(&copilot_token)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("editor-version", format!("vscode/{}", vscode_version))
        .header("editor-plugin-version", editor_plugin_version())
        .header("user-agent", user_agent())
        .header("copilot-integration-id", "vscode-chat")
        .header("x-github-api-version", "2025-04-01")
        .json(&payload)
        .send()
        .await
        .context("请求 embeddings 失败")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("embeddings 请求失败：{}", text);
    }

    resp.json::<EmbeddingResponse>().await.context("解析 embeddings 响应失败")
}
