use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── 请求类型 ────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct MessagesPayload {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    pub system: Option<Value>,          // string 或 Array<TextBlock>
    pub stop_sequences: Option<Vec<String>>,
    pub stream: Option<bool>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub tools: Option<Vec<AnthropicTool>>,
    pub tool_choice: Option<AnthropicToolChoice>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,                   // "user" | "assistant"
    pub content: Value,                 // string 或 Array<ContentBlock>
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AnthropicToolChoice {
    #[serde(rename = "type")]
    pub kind: String,                   // "auto" | "any" | "tool" | "none"
    pub name: Option<String>,
}

// ── 响应类型 ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MessagesResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub role: String,
    pub model: String,
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

// ── 流式事件类型 ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    MessageStart {
        message: MessageStartData,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDeltaData,
        usage: StreamUsage,
    },
    MessageStop,
    Ping,
    Error {
        error: StreamError,
    },
}

#[derive(Debug, Serialize)]
pub struct MessageStartData {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub role: String,
    pub model: String,
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Serialize)]
pub struct MessageDeltaData {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StreamUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

// ── 流式状态 ─────────────────────────────────────────────────

/// 流式翻译过程中的可变状态
pub struct StreamState {
    pub message_start_sent: bool,
    pub content_block_index: usize,
    pub content_block_open: bool,
    /// key: OpenAI tool_call index，value: (id, name, anthropic_block_index)
    pub tool_calls: std::collections::HashMap<usize, (String, String, usize)>,
}

impl StreamState {
    pub fn new() -> Self {
        Self {
            message_start_sent: false,
            content_block_index: 0,
            content_block_open: false,
            tool_calls: std::collections::HashMap::new(),
        }
    }

    /// 当前块是否是打开中的 tool_use 块
    pub fn is_tool_block_open(&self) -> bool {
        if !self.content_block_open {
            return false;
        }
        self.tool_calls
            .values()
            .any(|(_, _, idx)| *idx == self.content_block_index)
    }
}
