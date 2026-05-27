use serde_json::json;

use crate::copilot::chat::ChatCompletionChunk;

use super::translate::map_stop_reason;
use super::types::{ContentBlock, ContentDelta, StreamEvent, StreamState, StreamUsage};

/// Translate one OpenAI chunk into one or more Anthropic SSE events
pub fn translate_chunk(chunk: &ChatCompletionChunk, state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    let Some(choice) = chunk.choices.first() else {
        return events;
    };
    let delta = &choice.delta;

    // Send message_start on the first chunk
    if !state.message_start_sent {
        let (input_tokens, cache_read) = extract_input_usage(chunk);
        events.push(StreamEvent::MessageStart {
            message: super::types::MessageStartData {
                id: chunk.id.clone(),
                kind: "message".to_string(),
                role: "assistant".to_string(),
                model: chunk.model.clone(),
                content: vec![],
                stop_reason: None,
                stop_sequence: None,
                usage: super::types::AnthropicUsage {
                    input_tokens,
                    output_tokens: 0,
                    cache_read_input_tokens: cache_read,
                },
            },
        });
        state.message_start_sent = true;
    }

    // Handle text delta.
    if let Some(content_str) = delta.get("content").and_then(|v| v.as_str()) {
        if !content_str.is_empty() {
            // Close the current block first if it is a tool block
            if state.is_tool_block_open() {
                events.push(StreamEvent::ContentBlockStop {
                    index: state.content_block_index,
                });
                state.content_block_index += 1;
                state.content_block_open = false;
            }
            // Open a new text block
            if !state.content_block_open {
                events.push(StreamEvent::ContentBlockStart {
                    index: state.content_block_index,
                    content_block: ContentBlock::Text {
                        text: String::new(),
                    },
                });
                state.content_block_open = true;
            }
            events.push(StreamEvent::ContentBlockDelta {
                index: state.content_block_index,
                delta: ContentDelta::TextDelta {
                    text: content_str.to_string(),
                },
            });
        }
    }

    // Handle tool_calls delta
    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tool_calls {
            let tc_index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let tc_id = tc.get("id").and_then(|v| v.as_str()).map(String::from);
            let tc_name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let tc_args = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .map(String::from);

            // New tool call starting (has both id and name)
            if let (Some(id), Some(name)) = (tc_id, tc_name) {
                if state.content_block_open {
                    events.push(StreamEvent::ContentBlockStop {
                        index: state.content_block_index,
                    });
                    state.content_block_index += 1;
                    state.content_block_open = false;
                }
                let block_index = state.content_block_index;
                state
                    .tool_calls
                    .insert(tc_index, (id.clone(), name.clone(), block_index));
                events.push(StreamEvent::ContentBlockStart {
                    index: block_index,
                    content_block: ContentBlock::ToolUse {
                        id,
                        name,
                        input: json!({}),
                    },
                });
                state.content_block_open = true;
            }

            // Incremental tool arguments
            if let Some(args) = tc_args {
                if !args.is_empty() {
                    if let Some((_, _, block_idx)) = state.tool_calls.get(&tc_index) {
                        events.push(StreamEvent::ContentBlockDelta {
                            index: *block_idx,
                            delta: ContentDelta::InputJsonDelta { partial_json: args },
                        });
                    }
                }
            }
        }
    }

    // Handle finish_reason
    if let Some(finish_reason) = choice.finish_reason.as_deref() {
        if state.content_block_open {
            events.push(StreamEvent::ContentBlockStop {
                index: state.content_block_index,
            });
            state.content_block_open = false;
        }

        let (input_tokens, cache_read) = extract_input_usage(chunk);
        let output_tokens = chunk
            .usage
            .as_ref()
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        events.push(StreamEvent::MessageDelta {
            delta: super::types::MessageDeltaData {
                stop_reason: map_stop_reason(Some(finish_reason)),
                stop_sequence: None,
            },
            usage: StreamUsage {
                input_tokens,
                output_tokens,
                cache_read_input_tokens: cache_read,
            },
        });
        events.push(StreamEvent::MessageStop);
    }

    events
}

fn extract_input_usage(chunk: &ChatCompletionChunk) -> (u32, Option<u32>) {
    let usage = match &chunk.usage {
        Some(u) => u,
        None => return (0, None),
    };
    let prompt = usage
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cached = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let input = prompt.saturating_sub(cached);
    let cache_read = if cached > 0 { Some(cached) } else { None };
    (input, cache_read)
}
