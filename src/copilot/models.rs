use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::api::{editor_plugin_version, user_agent};
use crate::state::AppState;

/// Top-level container for the model list returned by Copilot
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelsResponse {
    pub data: Vec<Model>,
    pub object: String,
}

/// Single model entry; unused fields are stored in extra as Value to avoid serde flatten bugs
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub object: String,
    pub vendor: String,
    pub version: String,
    pub preview: bool,
    pub model_picker_enabled: bool,
    pub capabilities: ModelCapabilities,
    /// Absorbs model_picker_category, supported_endpoints, policy, and similar fields
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelCapabilities {
    pub family: String,
    pub object: String,
    pub tokenizer: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// limits is absent for some models (e.g. embedding models)
    pub limits: Option<ModelLimits>,
    pub supports: ModelSupports,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelLimits {
    pub max_context_window_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub max_prompt_tokens: Option<u32>,
    /// Absorbs max_non_streaming_output_tokens, vision, and similar fields
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelSupports {
    pub tool_calls: Option<bool>,
    pub parallel_tool_calls: Option<bool>,
    pub dimensions: Option<bool>,
    /// Absorbs streaming, vision, adaptive_thinking, reasoning_effort, and similar fields
    #[serde(flatten)]
    pub extra: Value,
}

/// Fetch the list of available models from the Copilot API
pub async fn get_models(client: &reqwest::Client, state: &AppState) -> Result<ModelsResponse> {
    let copilot_token = state
        .copilot_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Copilot Token is not set"))?;

    let vscode_version = state.vscode_version.as_ref();

    let resp = client
        .get("https://api.githubcopilot.com/models")
        .bearer_auth(&copilot_token)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("editor-version", format!("vscode/{}", vscode_version))
        .header("editor-plugin-version", editor_plugin_version())
        .header("user-agent", user_agent())
        .header("copilot-integration-id", "vscode-chat")
        .header("x-github-api-version", "2025-04-01")
        .send()
        .await
        .context("failed to request model list")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to fetch model list: {}", text);
    }

    // Fetch raw bytes for debugging; raw content is visible when parsing fails
    let bytes = resp
        .bytes()
        .await
        .context("failed to read model list response")?;
    serde_json::from_slice::<ModelsResponse>(&bytes).map_err(|e| {
        // Print a raw JSON snippet on parse failure to help locate field issues
        let preview: String = String::from_utf8_lossy(&bytes).chars().take(500).collect();
        anyhow::anyhow!(
            "failed to parse model list: {}\nfirst 500 response characters: {}",
            e,
            preview
        )
    })
}
