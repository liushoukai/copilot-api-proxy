use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::api::{editor_plugin_version, user_agent};
use crate::state::AppState;

/// Copilot 返回的模型列表（顶层容器）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelsResponse {
    pub data: Vec<Model>,
    pub object: String,
}

/// 单个模型，未使用的字段全部放入 extra 用 Value 存储，避免 serde flatten bug
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
    /// 吸收 model_picker_category、supported_endpoints、policy 等字段
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
    /// limits 在部分模型（如 embedding）中缺失
    pub limits: Option<ModelLimits>,
    pub supports: ModelSupports,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelLimits {
    pub max_context_window_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub max_prompt_tokens: Option<u32>,
    /// 吸收 max_non_streaming_output_tokens、vision 等字段
    #[serde(flatten)]
    pub extra: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelSupports {
    pub tool_calls: Option<bool>,
    pub parallel_tool_calls: Option<bool>,
    pub dimensions: Option<bool>,
    /// 吸收 streaming、vision、adaptive_thinking、reasoning_effort 等字段
    #[serde(flatten)]
    pub extra: Value,
}

/// 从 Copilot API 获取可用模型列表
pub async fn get_models(client: &reqwest::Client, state: &AppState) -> Result<ModelsResponse> {
    let copilot_token = state
        .copilot_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Copilot Token 未设置"))?;

    let vscode_version = state.vscode_version.read().await.clone();

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
        .context("请求模型列表失败")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("获取模型列表失败：{}", text);
    }

    // 先拿原始 bytes 做调试，解析失败时能看到具体内容
    let bytes = resp.bytes().await.context("读取模型列表响应失败")?;
    serde_json::from_slice::<ModelsResponse>(&bytes).map_err(|e| {
        // 打印解析失败的原始 JSON 片段，方便定位字段问题
        let preview: String = String::from_utf8_lossy(&bytes).chars().take(500).collect();
        anyhow::anyhow!("解析模型列表失败：{}\n响应前500字符：{}", e, preview)
    })
}
