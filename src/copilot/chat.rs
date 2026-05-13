use anyhow::{Context, Result};
use axum::body::Body;
use reqwest::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::api::{editor_plugin_version, user_agent};
use crate::state::AppState;

// ── 请求类型 ────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ChatCompletionsPayload {
    pub messages: Vec<Message>,
    pub model: String,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stop: Option<Value>,
    pub n: Option<u32>,
    pub stream: Option<bool>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub logit_bias: Option<Value>,
    pub logprobs: Option<bool>,
    pub response_format: Option<Value>,
    pub seed: Option<i64>,
    pub tools: Option<Vec<Tool>>,
    pub tool_choice: Option<Value>,
    pub user: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: Option<Value>,
    pub name: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionDef,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionCall,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ── 非流式响应类型 ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChoice {
    pub finish_reason: Option<String>,
    pub message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub content: Option<Value>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

// ── 流式 chunk 类型 ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    pub usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct ChunkChoice {
    pub delta: Value,
    pub finish_reason: Option<String>,
}

// ── 核心调用逻辑 ────────────────────────────────────────────

/// 向 Copilot API 发起 chat/completions 请求，返回原始 Response（支持流式透传）
pub async fn create_chat_completions(
    client: &reqwest::Client,
    state: &AppState,
    payload: ChatCompletionsPayload,
) -> Result<Response> {
    let copilot_token = state
        .copilot_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Copilot Token 未设置"))?;

    let vscode_version = state.vscode_version.as_ref();

    // 判断是否为 agent 调用（消息中有 assistant / tool 角色）
    let is_agent = payload
        .messages
        .iter()
        .any(|m| m.role == "assistant" || m.role == "tool");

    // 判断是否包含图片内容（vision 请求）
    let enable_vision = payload.messages.iter().any(|m| {
        m.content
            .as_ref()
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().any(|p| {
                p.get("type").and_then(|t| t.as_str()) == Some("image_url")
            }))
            .unwrap_or(false)
    });

    let mut req = client
        .post("https://api.githubcopilot.com/chat/completions")
        .bearer_auth(&copilot_token)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("editor-version", format!("vscode/{}", vscode_version))
        .header("editor-plugin-version", editor_plugin_version())
        .header("user-agent", user_agent())
        .header("copilot-integration-id", "vscode-chat")
        .header("openai-intent", "conversation-panel")
        .header("x-github-api-version", "2025-04-01")
        .header("x-vscode-user-agent-library-version", "electron-fetch")
        .header("x-initiator", if is_agent { "agent" } else { "user" })
        .json(&payload);

    if enable_vision {
        req = req.header("copilot-vision-request", "true");
    }

    req.send().await.context("请求 chat/completions 失败")
}

/// 将 reqwest 流式响应转换为 axum SSE Body
pub fn to_sse_body(resp: Response) -> Body {
    Body::from_stream(resp.bytes_stream())
}
