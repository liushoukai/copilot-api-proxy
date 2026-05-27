use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::api::{editor_plugin_version, user_agent};
use crate::state::AppState;

// ── Request / Response types ─────────────────────────────────────────

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

/// Send an embeddings request to the Copilot API
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
        .ok_or_else(|| anyhow::anyhow!("Copilot Token is not set"))?;

    let vscode_version = state.vscode_version.as_ref();

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
        .context("failed to request embeddings")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("embeddings request failed: {}", text);
    }

    resp.json::<EmbeddingResponse>()
        .await
        .context("failed to parse embeddings response")
}
