use serde_json::{Value, json};

use crate::copilot::chat::{
    ChatCompletionsPayload, ChatCompletionResponse, Message, Tool, FunctionDef,
};

use super::types::{
    AnthropicMessage, AnthropicTool, AnthropicToolChoice, MessagesPayload,
    MessagesResponse, AnthropicUsage,
};

// ── 模型名称规范化 ────────────────────────────────────────────

/// 基于 Copilot 实际可用模型列表，将请求的模型名映射到最合适的模型 ID。
///
/// 匹配策略（按优先级）：
/// 1. 精确匹配 model_id
/// 2. 去掉日期后缀后精确匹配
/// 3. family 匹配（claude-sonnet → 取最新同 family 模型）
/// 4. 同代降级（haiku → sonnet，同 major 版本）
/// 5. 从列表中选第一个 claude chat 模型兜底
/// 6. 若列表为空，返回原始名称不做修改
pub fn resolve_model<'a>(requested: &str, available: &'a [String]) -> String {
    if available.is_empty() {
        return requested.to_string();
    }

    // 步骤1：精确匹配
    if available.iter().any(|id| id == requested) {
        return requested.to_string();
    }

    // 步骤2：去掉日期后缀后精确匹配（claude-haiku-4-5-20251001 → claude-haiku-4-5）
    let without_date = strip_date_suffix(requested);
    if available.iter().any(|id| id == &without_date) {
        return without_date;
    }

    // 步骤3：版本点格式匹配（claude-sonnet-4-5 → claude-sonnet-4.5）
    let dotted = normalize_version(requested);
    if available.iter().any(|id| id == &dotted) {
        return dotted.clone();
    }

    // 步骤4：family + major 匹配，取最新版本
    // 例：claude-sonnet-4.6 → family="sonnet", major=4，找 claude-sonnet-4.*
    if let Some(best) = find_by_family_major(&dotted, available) {
        return best;
    }

    // 步骤5：haiku 降级到同代 sonnet（claude-haiku-4.5 → claude-sonnet-4.5 或同代 sonnet）
    if dotted.contains("-haiku-") {
        let sonnet_name = dotted.replace("-haiku-", "-sonnet-");
        if let Some(best) = find_by_family_major(&sonnet_name, available) {
            return best;
        }
    }

    // 步骤6：从列表中取第一个 claude chat 模型兜底
    if let Some(fallback) = available.iter().find(|id| id.starts_with("claude-")) {
        return fallback.clone();
    }

    // 步骤7：实在没有合适的，原样返回
    requested.to_string()
}

/// 去掉末尾的日期后缀（纯数字且长度>=6），如 claude-haiku-4-5-20251001 → claude-haiku-4-5
fn strip_date_suffix(model: &str) -> String {
    let parts: Vec<&str> = model.split('-').collect();
    let has_date = parts.last()
        .map(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() >= 6)
        .unwrap_or(false);
    if has_date && parts.len() > 1 {
        parts[..parts.len() - 1].join("-")
    } else {
        model.to_string()
    }
}

/// 将 claude-{family}-{major}-{minor}[-{date}] 转为 claude-{family}-{major}.{minor}
fn normalize_version(model: &str) -> String {
    let parts: Vec<&str> = model.split('-').collect();
    if parts.len() < 4 || parts[0] != "claude" {
        return model.to_string();
    }

    // 末尾是日期（纯数字，长度>=6）则去掉，再取最后一段作 minor
    let end = if parts.last().map(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() >= 6).unwrap_or(false) {
        parts.len() - 1
    } else {
        parts.len()
    };

    let minor = parts[end - 1];
    // minor 必须是纯数字
    if !minor.chars().all(|c| c.is_ascii_digit()) {
        return model.to_string();
    }

    let prefix = parts[..end - 1].join("-");
    format!("{}.{}", prefix, minor)
}

/// 从 available 列表中找与 model 同 family + major 的最新版本。
/// 例：dotted="claude-sonnet-4.6"，family="sonnet"，major="4"
/// 会在列表中找所有 "claude-sonnet-4.*" 并返回最后一个（ID 通常按版本排序）。
fn find_by_family_major(model: &str, available: &[String]) -> Option<String> {
    // 解析 claude-{family}-{major}.{minor} 格式
    let parts: Vec<&str> = model.splitn(2, '-').collect();
    if parts.len() < 2 || parts[0] != "claude" {
        return None;
    }
    // prefix = "claude-sonnet-4"（取到 major）
    let prefix = {
        // 找最后一个 '.' 前的部分作为前缀匹配键
        let dot_pos = model.rfind('.')?;
        &model[..dot_pos]
    };

    let matches: Vec<&String> = available.iter()
        .filter(|id| id.starts_with(prefix))
        .collect();

    matches.last().map(|s| (*s).clone())
}

// ── Anthropic → OpenAI 请求转换 ───────────────────────────────

/// 将 Anthropic 请求格式转换为 OpenAI 格式，使用 available_models 做动态模型映射
pub fn translate_to_openai(payload: &MessagesPayload, available_models: &[String]) -> ChatCompletionsPayload {
    ChatCompletionsPayload {
        model: resolve_model(&payload.model, available_models),
        messages: translate_messages(&payload.messages, &payload.system),
        max_tokens: Some(payload.max_tokens),
        stop: payload.stop_sequences.clone().map(Value::from),
        stream: payload.stream,
        temperature: payload.temperature,
        top_p: payload.top_p,
        user: payload.metadata.as_ref()
            .and_then(|m| m.get("user_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        tools: translate_tools(payload.tools.as_deref()),
        tool_choice: translate_tool_choice(payload.tool_choice.as_ref()),
        ..Default::default()
    }
}

fn translate_messages(
    messages: &[AnthropicMessage],
    system: &Option<Value>,
) -> Vec<Message> {
    let mut result = Vec::new();

    // system prompt 转换为 OpenAI system 消息
    if let Some(sys) = system {
        let text = match sys {
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr.iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n\n"),
            _ => String::new(),
        };
        if !text.is_empty() {
            result.push(Message {
                role: "system".to_string(),
                content: Some(Value::String(text)),
                ..Default::default()
            });
        }
    }

    for msg in messages {
        result.extend(translate_message(msg));
    }
    result
}

fn translate_message(msg: &AnthropicMessage) -> Vec<Message> {
    if msg.role == "user" {
        translate_user_message(&msg.content)
    } else {
        translate_assistant_message(&msg.content)
    }
}

fn translate_user_message(content: &Value) -> Vec<Message> {
    let mut result = Vec::new();

    let blocks = match content {
        Value::Array(arr) => arr.as_slice(),
        _ => {
            // 纯文本直接返回
            return vec![Message {
                role: "user".to_string(),
                content: Some(content.clone()),
                ..Default::default()
            }];
        }
    };

    // tool_result 块转为 OpenAI tool 消息
    for block in blocks.iter().filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result")) {
        result.push(Message {
            role: "tool".to_string(),
            tool_call_id: block.get("tool_use_id").and_then(|v| v.as_str()).map(String::from),
            content: block.get("content").cloned(),
            ..Default::default()
        });
    }

    // 其余块保留为 user 消息
    let other_blocks: Vec<&Value> = blocks.iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) != Some("tool_result"))
        .collect();

    if !other_blocks.is_empty() {
        let mapped = map_content_blocks(&other_blocks);
        result.push(Message {
            role: "user".to_string(),
            content: Some(mapped),
            ..Default::default()
        });
    }

    result
}

fn translate_assistant_message(content: &Value) -> Vec<Message> {
    let blocks = match content {
        Value::Array(arr) => arr,
        _ => {
            return vec![Message {
                role: "assistant".to_string(),
                content: Some(content.clone()),
                ..Default::default()
            }]
        }
    };

    let tool_use_blocks: Vec<&Value> = blocks.iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .collect();

    let text_parts: Vec<&str> = blocks.iter()
        .filter(|b| matches!(b.get("type").and_then(|t| t.as_str()), Some("text") | Some("thinking")))
        .filter_map(|b| {
            if b.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                b.get("thinking").and_then(|v| v.as_str())
            } else {
                b.get("text").and_then(|v| v.as_str())
            }
        })
        .collect();

    let text = text_parts.join("\n\n");

    if !tool_use_blocks.is_empty() {
        let tool_calls: Vec<Value> = tool_use_blocks.iter().map(|b| json!({
            "id": b.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "type": "function",
            "function": {
                "name": b.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "arguments": serde_json::to_string(b.get("input").unwrap_or(&Value::Null)).unwrap_or_default(),
            }
        })).collect();

        vec![Message {
            role: "assistant".to_string(),
            content: if text.is_empty() { None } else { Some(Value::String(text)) },
            tool_calls: Some(tool_calls.into_iter()
                .map(|v| serde_json::from_value(v).unwrap())
                .collect()),
            ..Default::default()
        }]
    } else {
        vec![Message {
            role: "assistant".to_string(),
            content: Some(Value::String(text)),
            ..Default::default()
        }]
    }
}

/// 将内容块列表映射为 OpenAI content：有图片则返回数组，否则拼接纯文本
fn map_content_blocks(blocks: &[&Value]) -> Value {
    let has_image = blocks.iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("image"));

    if !has_image {
        let text: String = blocks.iter()
            .filter_map(|b| {
                let kind = b.get("type").and_then(|t| t.as_str())?;
                match kind {
                    "text" => b.get("text").and_then(|v| v.as_str()),
                    "thinking" => b.get("thinking").and_then(|v| v.as_str()),
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        return Value::String(text);
    }

    // 有图片：转换为 OpenAI 多模态数组
    let parts: Vec<Value> = blocks.iter().filter_map(|b| {
        match b.get("type").and_then(|t| t.as_str())? {
            "text" => Some(json!({ "type": "text", "text": b.get("text").and_then(|v| v.as_str()).unwrap_or("") })),
            "thinking" => Some(json!({ "type": "text", "text": b.get("thinking").and_then(|v| v.as_str()).unwrap_or("") })),
            "image" => {
                let source = b.get("source")?;
                let media_type = source.get("media_type").and_then(|v| v.as_str())?;
                let data = source.get("data").and_then(|v| v.as_str())?;
                Some(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{};base64,{}", media_type, data) }
                }))
            }
            _ => None,
        }
    }).collect();

    Value::Array(parts)
}

fn translate_tools(tools: Option<&[AnthropicTool]>) -> Option<Vec<Tool>> {
    tools.map(|ts| ts.iter().map(|t| Tool {
        kind: "function".to_string(),
        function: FunctionDef {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        },
    }).collect())
}

fn translate_tool_choice(choice: Option<&AnthropicToolChoice>) -> Option<Value> {
    choice.map(|c| match c.kind.as_str() {
        "auto" => Value::String("auto".to_string()),
        "any" => Value::String("required".to_string()),
        "none" => Value::String("none".to_string()),
        "tool" => json!({
            "type": "function",
            "function": { "name": c.name.as_deref().unwrap_or("") }
        }),
        _ => Value::Null,
    })
}

// ── OpenAI → Anthropic 响应转换 ───────────────────────────────

pub fn translate_to_anthropic(resp: &ChatCompletionResponse) -> MessagesResponse {
    let mut text_blocks: Vec<Value> = Vec::new();
    let mut tool_use_blocks: Vec<Value> = Vec::new();
    let mut stop_reason = None;

    for choice in &resp.choices {
        stop_reason = choice.finish_reason.clone();

        if let Some(ref content) = choice.message.content {
            if let Some(text) = content.as_str() {
                if !text.is_empty() {
                    text_blocks.push(json!({ "type": "text", "text": text }));
                }
            }
        }

        if let Some(ref tool_calls) = choice.message.tool_calls {
            for tc in tool_calls {
                let input: Value = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or(Value::Null);
                tool_use_blocks.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.function.name,
                    "input": input,
                }));
            }
        }
    }

    let mut content = text_blocks;
    content.extend(tool_use_blocks);

    let (input_tokens, output_tokens, cache_read) = resp.usage.as_ref().map(|u| {
        let prompt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let completion = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let cached = u.get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let input = prompt.saturating_sub(cached);
        let cache_read = if cached > 0 { Some(cached) } else { None };
        (input, completion, cache_read)
    }).unwrap_or((0, 0, None));

    MessagesResponse {
        id: resp.id.clone(),
        kind: "message".to_string(),
        role: "assistant".to_string(),
        model: resp.model.clone(),
        content,
        stop_reason: map_stop_reason(stop_reason.as_deref()),
        stop_sequence: None,
        usage: AnthropicUsage { input_tokens, output_tokens, cache_read_input_tokens: cache_read },
    }
}

pub fn map_stop_reason(reason: Option<&str>) -> Option<String> {
    reason.map(|r| match r {
        "stop" => "end_turn",
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        "content_filter" => "end_turn",
        _ => "end_turn",
    }.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_exact_match() {
        let available = models(&["claude-3.5-sonnet", "claude-3.7-sonnet", "gpt-4o"]);
        assert_eq!(resolve_model("claude-3.5-sonnet", &available), "claude-3.5-sonnet");
    }

    #[test]
    fn test_date_suffix_stripped() {
        // claude-haiku-4-5-20251001 → 去日期 → claude-haiku-4-5→ 无点格式无精确匹配
        // → dotted = claude-haiku-4.5 → 无精确匹配 → family+major = claude-haiku-4 → 无
        // → haiku降级 → claude-sonnet-4.5 → family+major = claude-sonnet-4 → claude-3.5-sonnet 不满足
        // → fallback 第一个 claude
        let available = models(&["claude-3.5-sonnet", "gpt-4o"]);
        let result = resolve_model("claude-haiku-4-5-20251001", &available);
        assert_eq!(result, "claude-3.5-sonnet");
    }

    #[test]
    fn test_dotted_exact_match() {
        let available = models(&["claude-sonnet-4.5", "claude-sonnet-4.6"]);
        assert_eq!(resolve_model("claude-sonnet-4-5", &available), "claude-sonnet-4.5");
    }

    #[test]
    fn test_family_major_match() {
        // claude-sonnet-4.6 → prefix=claude-sonnet-4 → 匹配 claude-sonnet-4.5
        let available = models(&["claude-sonnet-4.5", "gpt-4o"]);
        assert_eq!(resolve_model("claude-sonnet-4.6", &available), "claude-sonnet-4.5");
    }

    #[test]
    fn test_haiku_downgrade() {
        // haiku-4.5 → sonnet降级 → claude-sonnet-4.5 精确匹配
        let available = models(&["claude-sonnet-4.5", "gpt-4o"]);
        assert_eq!(resolve_model("claude-haiku-4.5", &available), "claude-sonnet-4.5");
    }

    #[test]
    fn test_empty_list() {
        assert_eq!(resolve_model("claude-sonnet-4.6", &[]), "claude-sonnet-4.6");
    }

    #[test]
    fn test_fallback_first_claude() {
        let available = models(&["claude-3.5-sonnet", "gpt-4o"]);
        // 请求一个完全不匹配的型号
        let result = resolve_model("claude-opus-99.9", &available);
        assert_eq!(result, "claude-3.5-sonnet");
    }
}
