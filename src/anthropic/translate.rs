use serde_json::{Value, json};

use crate::copilot::chat::{
    ChatCompletionResponse, ChatCompletionsPayload, FunctionDef, Message, Tool,
};

use super::types::{
    AnthropicMessage, AnthropicTool, AnthropicToolChoice, AnthropicUsage, MessagesPayload,
    MessagesResponse,
};

// ── Model name normalization ────────────────────────────────────────────

/// Map the requested model name to the best matching model ID from the Copilot available model list.
///
/// Matching strategy (in priority order):
/// 1. Exact match on model_id
/// 2. Exact match after stripping date suffix
/// 3. Family match (claude-sonnet → pick the newest model of the same family)
/// 4. Same-generation downgrade (haiku → sonnet, same major version)
/// 5. Fall back to the first claude model in the list
/// 6. Return the original name unchanged if the list is empty
pub fn resolve_model<'a>(requested: &str, available: &'a [String]) -> String {
    if available.is_empty() {
        return requested.to_string();
    }

    // Step 1: exact match
    if available.iter().any(|id| id == requested) {
        return requested.to_string();
    }

    // Step 2: exact match after stripping date suffix (claude-haiku-4-5-20251001 → claude-haiku-4-5)
    let without_date = strip_date_suffix(requested);
    if available.iter().any(|id| id == &without_date) {
        return without_date;
    }

    // Step 3: dotted version format match (claude-sonnet-4-5 → claude-sonnet-4.5)
    let dotted = normalize_version(requested);
    if available.iter().any(|id| id == &dotted) {
        return dotted.clone();
    }

    // Step 4: family + major match, pick the newest version
    // e.g. claude-sonnet-4.6 → family="sonnet", major=4, look for claude-sonnet-4.*
    if let Some(best) = find_by_family_major(&dotted, available) {
        return best;
    }

    // Step 5: downgrade haiku to same-generation sonnet (claude-haiku-4.5 → claude-sonnet-4.5 or same-gen sonnet)
    if dotted.contains("-haiku-") {
        let sonnet_name = dotted.replace("-haiku-", "-sonnet-");
        if let Some(best) = find_by_family_major(&sonnet_name, available) {
            return best;
        }
    }

    // Step 6: fall back to the first claude model in the list
    if let Some(fallback) = available.iter().find(|id| id.starts_with("claude-")) {
        return fallback.clone();
    }

    // Step 7: no suitable match found, return as-is
    requested.to_string()
}

/// Strip trailing date suffix (all-digit segment with length >= 6), e.g. claude-haiku-4-5-20251001 → claude-haiku-4-5
fn strip_date_suffix(model: &str) -> String {
    let parts: Vec<&str> = model.split('-').collect();
    let has_date = parts
        .last()
        .map(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() >= 6)
        .unwrap_or(false);
    if has_date && parts.len() > 1 {
        parts[..parts.len() - 1].join("-")
    } else {
        model.to_string()
    }
}

/// Convert claude-{family}-{major}-{minor}[-{date}] to claude-{family}-{major}.{minor}
fn normalize_version(model: &str) -> String {
    let parts: Vec<&str> = model.split('-').collect();
    if parts.len() < 4 || parts[0] != "claude" {
        return model.to_string();
    }

    // Drop trailing date segment (all-digit, length >= 6), then use the last segment as minor
    let end = if parts
        .last()
        .map(|p| p.chars().all(|c| c.is_ascii_digit()) && p.len() >= 6)
        .unwrap_or(false)
    {
        parts.len() - 1
    } else {
        parts.len()
    };

    let minor = parts[end - 1];
    // minor must be all digits
    if !minor.chars().all(|c| c.is_ascii_digit()) {
        return model.to_string();
    }

    let prefix = parts[..end - 1].join("-");
    format!("{}.{}", prefix, minor)
}

/// Find the newest model in available with the same family + major as model.
/// e.g. dotted="claude-sonnet-4.6", family="sonnet", major="4"
/// finds all "claude-sonnet-4.*" entries and returns the last one (IDs are typically sorted by version).
fn find_by_family_major(model: &str, available: &[String]) -> Option<String> {
    // Parse claude-{family}-{major}.{minor} format
    let parts: Vec<&str> = model.splitn(2, '-').collect();
    if parts.len() < 2 || parts[0] != "claude" {
        return None;
    }
    // prefix = "claude-sonnet-4" (up to and including major)
    let prefix = {
        // Use the substring before the last '.' as the prefix matching key
        let dot_pos = model.rfind('.')?;
        &model[..dot_pos]
    };

    let matches: Vec<&String> = available
        .iter()
        .filter(|id| id.starts_with(prefix))
        .collect();

    matches.last().map(|s| (*s).clone())
}

// ── Anthropic → OpenAI request translation ───────────────────────────────

/// Convert an Anthropic request into OpenAI format, using available_models for dynamic model mapping.
pub fn translate_to_openai(
    payload: &MessagesPayload,
    available_models: &[String],
) -> ChatCompletionsPayload {
    ChatCompletionsPayload {
        model: resolve_model(&payload.model, available_models),
        messages: translate_messages(&payload.messages, &payload.system),
        max_tokens: Some(payload.max_tokens),
        stop: payload.stop_sequences.clone().map(Value::from),
        stream: payload.stream,
        temperature: payload.temperature,
        top_p: payload.top_p,
        user: payload
            .metadata
            .as_ref()
            .and_then(|m| m.get("user_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        tools: translate_tools(payload.tools.as_deref()),
        tool_choice: translate_tool_choice(payload.tool_choice.as_ref()),
        ..Default::default()
    }
}

fn translate_messages(messages: &[AnthropicMessage], system: &Option<Value>) -> Vec<Message> {
    let mut result = Vec::new();

    // Convert the system prompt into an OpenAI system message.
    if let Some(sys) = system {
        let text = match sys {
            Value::String(s) => s.clone(),
            Value::Array(arr) => arr
                .iter()
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
            // Return plain text directly.
            return vec![Message {
                role: "user".to_string(),
                content: Some(content.clone()),
                ..Default::default()
            }];
        }
    };

    // Convert tool_result blocks into OpenAI tool messages.
    for block in blocks
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
    {
        result.push(Message {
            role: "tool".to_string(),
            tool_call_id: block
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .map(String::from),
            content: block.get("content").cloned(),
            ..Default::default()
        });
    }

    // Keep all other blocks as user messages.
    let other_blocks: Vec<&Value> = blocks
        .iter()
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
            }];
        }
    };

    let tool_use_blocks: Vec<&Value> = blocks
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .collect();

    let text_parts: Vec<&str> = blocks
        .iter()
        .filter(|b| {
            matches!(
                b.get("type").and_then(|t| t.as_str()),
                Some("text") | Some("thinking")
            )
        })
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
            content: if text.is_empty() {
                None
            } else {
                Some(Value::String(text))
            },
            tool_calls: Some(
                tool_calls
                    .into_iter()
                    .filter_map(|v| match serde_json::from_value(v.clone()) {
                        Ok(tc) => Some(tc),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to deserialize tool_call; skipped: {} | raw data: {}",
                                e,
                                v
                            );
                            None
                        }
                    })
                    .collect(),
            ),
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

/// Map content blocks to OpenAI content: return an array for images, otherwise concatenate plain text.
fn map_content_blocks(blocks: &[&Value]) -> Value {
    let has_image = blocks
        .iter()
        .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("image"));

    if !has_image {
        let text: String = blocks
            .iter()
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

    // Images are present: convert to an OpenAI multimodal array.
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
    tools.map(|ts| {
        ts.iter()
            .map(|t| Tool {
                kind: "function".to_string(),
                function: FunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    })
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

// ── OpenAI → Anthropic response translation ───────────────────────────────

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
                let input: Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
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

    let (input_tokens, output_tokens, cache_read) = resp
        .usage
        .as_ref()
        .map(|u| {
            let prompt = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let completion = u
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let cached = u
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let input = prompt.saturating_sub(cached);
            let cache_read = if cached > 0 { Some(cached) } else { None };
            (input, completion, cache_read)
        })
        .unwrap_or((0, 0, None));

    MessagesResponse {
        id: resp.id.clone(),
        kind: "message".to_string(),
        role: "assistant".to_string(),
        model: resp.model.clone(),
        content,
        stop_reason: map_stop_reason(stop_reason.as_deref()),
        stop_sequence: None,
        usage: AnthropicUsage {
            input_tokens,
            output_tokens,
            cache_read_input_tokens: cache_read,
        },
    }
}

pub fn map_stop_reason(reason: Option<&str>) -> Option<String> {
    reason.map(|r| {
        match r {
            "stop" => "end_turn",
            "length" => "max_tokens",
            "tool_calls" => "tool_use",
            "content_filter" => "end_turn",
            _ => "end_turn",
        }
        .to_string()
    })
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
        assert_eq!(
            resolve_model("claude-3.5-sonnet", &available),
            "claude-3.5-sonnet"
        );
    }

    #[test]
    fn test_date_suffix_stripped() {
        // claude-haiku-4-5-20251001 -> strip date -> claude-haiku-4-5 -> no exact undotted match
        // -> dotted = claude-haiku-4.5 -> no exact match -> family+major = claude-haiku-4 -> none
        // -> haiku downgrade -> claude-sonnet-4.5 -> family+major = claude-sonnet-4 -> claude-3.5-sonnet does not match
        // -> fallback to the first claude model
        let available = models(&["claude-3.5-sonnet", "gpt-4o"]);
        let result = resolve_model("claude-haiku-4-5-20251001", &available);
        assert_eq!(result, "claude-3.5-sonnet");
    }

    #[test]
    fn test_dotted_exact_match() {
        let available = models(&["claude-sonnet-4.5", "claude-sonnet-4.6"]);
        assert_eq!(
            resolve_model("claude-sonnet-4-5", &available),
            "claude-sonnet-4.5"
        );
    }

    #[test]
    fn test_family_major_match() {
        // claude-sonnet-4.6 -> prefix=claude-sonnet-4 -> matches claude-sonnet-4.5
        let available = models(&["claude-sonnet-4.5", "gpt-4o"]);
        assert_eq!(
            resolve_model("claude-sonnet-4.6", &available),
            "claude-sonnet-4.5"
        );
    }

    #[test]
    fn test_haiku_downgrade() {
        // haiku-4.5 -> sonnet downgrade -> exact match on claude-sonnet-4.5
        let available = models(&["claude-sonnet-4.5", "gpt-4o"]);
        assert_eq!(
            resolve_model("claude-haiku-4.5", &available),
            "claude-sonnet-4.5"
        );
    }

    #[test]
    fn test_empty_list() {
        assert_eq!(resolve_model("claude-sonnet-4.6", &[]), "claude-sonnet-4.6");
    }

    #[test]
    fn test_fallback_first_claude() {
        let available = models(&["claude-3.5-sonnet", "gpt-4o"]);
        // Request a model that does not match anything.
        let result = resolve_model("claude-opus-99.9", &available);
        assert_eq!(result, "claude-3.5-sonnet");
    }
}
