use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;
use tokio_util::io::StreamReader;
use tracing::{error, info};

use crate::anthropic::stream::translate_chunk;
use crate::anthropic::translate::{translate_to_anthropic, translate_to_openai};
use crate::anthropic::types::{MessagesPayload, StreamEvent, StreamState};
use crate::copilot::chat::{
    ChatCompletionChunk, ChatCompletionResponse, ChatCompletionsPayload,
    create_chat_completions, to_sse_body,
};
use crate::copilot::embeddings::{EmbeddingRequest, create_embeddings};
use crate::copilot::models::get_models;
use crate::state::AppState;
use crate::token::refresh_copilot_token;

pub fn build_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/health", get(health_handler))
        .route("/models", get(models_handler))
        .route("/chat/completions", post(chat_completions_handler))
        .route("/embeddings", post(embeddings_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/embeddings", post(embeddings_handler))
        .route("/v1/messages", post(messages_handler))
        .layer(from_fn(log_request_size_middleware))
        .with_state(state.clone());

    router
}

/// 将字节数格式化为可读的大小字符串（B / KB / MB）
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 中间件：记录所有请求的路径和请求体大小
async fn log_request_size_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            error!("读取请求体失败：{}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };
    info!("{} {} 请求体大小：{}", method, path, format_size(bytes.len()));
    let req = Request::from_parts(parts, Body::from(bytes));
    next.run(req).await
}

async fn health_handler(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "github_token": state.github_token.read().await.is_some(),
        "copilot_token": state.copilot_token.read().await.is_some(),
    }))
}

async fn models_handler(State(state): State<AppState>) -> Response {
    // 优先使用启动时缓存的模型列表
    {
        let cached = state.models.read().await;
        if let Some(ref models_resp) = *cached {
            let data: Vec<Value> = models_resp.data.iter().map(model_to_json).collect();
            return Json(json!({ "object": "list", "data": data })).into_response();
        }
    }

    // 缓存不存在时兜底实时请求
    match get_models(&state.client, &state).await {
        Ok(resp) => {
            let data: Vec<Value> = resp.data.iter().map(model_to_json).collect();
            Json(json!({ "object": "list", "data": data })).into_response()
        }
        Err(e) => {
            error!("获取模型列表失败：{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn chat_completions_handler(
    State(state): State<AppState>,
    Json(mut payload): Json<ChatCompletionsPayload>,
) -> Response {
    let is_stream = payload.stream.unwrap_or(false);

    // 从缓存读取模型 ID 列表（Arc clone，无字符串拷贝）
    let available_models = state.model_ids.read().await.clone();
    let original_model = payload.model.clone();
    payload.model = crate::anthropic::translate::resolve_model(&payload.model, &available_models);
    if payload.model != original_model {
        info!("chat/completions → model: {} → {}", original_model, payload.model);
    }

    // 首次请求，clone 保留用于 401 重试
    let mut upstream_resp = match create_chat_completions(&state.client, &state, payload.clone()).await {
        Ok(r) => r,
        Err(e) => {
            error!("chat/completions 请求失败：{}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    // 400/401 时主动刷新 Copilot Token 并重试一次
    if is_auth_error(upstream_resp.status()) {
        info!("Copilot Token 认证失败（{}），主动刷新后重试...", upstream_resp.status());
        if let Err(e) = refresh_copilot_token(&state).await {
            error!("Copilot Token 刷新失败：{}", e);
            return (StatusCode::UNAUTHORIZED, upstream_resp.text().await.unwrap_or_default()).into_response();
        }
        upstream_resp = match create_chat_completions(&state.client, &state, payload).await {
            Ok(r) => r,
            Err(e) => {
                error!("chat/completions 重试请求失败：{}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };
    }

    if !upstream_resp.status().is_success() {
        let status = upstream_resp.status();
        let body = upstream_resp.text().await.unwrap_or_default();
        error!("Copilot chat/completions 返回错误 {}：{}", status, body);
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            body,
        ).into_response();
    }

    if is_stream {
        // 流式：把 Copilot SSE 字节流直接透传给客户端
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/event-stream"));
        headers.insert("cache-control", HeaderValue::from_static("no-cache"));
        headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
        return (headers, to_sse_body(upstream_resp)).into_response();
    }

    match upstream_resp.json::<Value>().await {
        Ok(json) => Json(json).into_response(),
        Err(e) => {
            error!("解析 chat/completions 响应失败：{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn embeddings_handler(
    State(state): State<AppState>,
    Json(payload): Json<EmbeddingRequest>,
) -> Response {
    match create_embeddings(&state.client, &state, payload).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            error!("embeddings 请求失败：{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// 400（token 格式异常）和 401（token 过期）均视为认证错误，触发主动刷新重试
fn is_auth_error(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST
}

fn model_to_json(m: &crate::copilot::models::Model) -> Value {
    json!({
        "id": m.id,
        "object": "model",
        "created": 0,
        "owned_by": m.vendor,
        "display_name": m.name,
    })
}

/// Anthropic Messages API 处理器，将请求翻译后转发给 Copilot，再将响应翻译回 Anthropic 格式
async fn messages_handler(
    State(state): State<AppState>,
    Json(payload): Json<MessagesPayload>,
) -> Response {
    let is_stream = payload.stream.unwrap_or(false);

    // 从缓存读取模型 ID 列表（Arc clone，无字符串拷贝）
    let available_models = state.model_ids.read().await.clone();
    let openai_payload = translate_to_openai(&payload, &available_models);
    info!("messages → model: {} → {}", payload.model, openai_payload.model);

    // 首次请求，clone 保留用于 401 重试
    let mut upstream_resp = match create_chat_completions(&state.client, &state, openai_payload.clone()).await {
        Ok(r) => r,
        Err(e) => {
            error!("messages 请求失败：{}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    // 400/401 时主动刷新 Copilot Token 并重试一次
    if is_auth_error(upstream_resp.status()) {
        info!("Copilot Token 认证失败（{}），主动刷新后重试...", upstream_resp.status());
        if let Err(e) = refresh_copilot_token(&state).await {
            error!("Copilot Token 刷新失败：{}", e);
            return (StatusCode::UNAUTHORIZED, upstream_resp.text().await.unwrap_or_default()).into_response();
        }
        upstream_resp = match create_chat_completions(&state.client, &state, openai_payload).await {
            Ok(r) => r,
            Err(e) => {
                error!("messages 重试请求失败：{}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };
    }

    if !upstream_resp.status().is_success() {
        let status = upstream_resp.status();
        let body = upstream_resp.text().await.unwrap_or_default();
        error!("Copilot chat/completions 返回错误 {}：{}", status, body);
        return (
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            body,
        ).into_response();
    }

    if is_stream {
        return messages_stream_response(upstream_resp).await;
    }

    messages_non_stream_response(upstream_resp).await
}

/// 非流式：将 OpenAI 响应解析后转换为 Anthropic MessagesResponse
async fn messages_non_stream_response(upstream_resp: reqwest::Response) -> Response {
    match upstream_resp.json::<ChatCompletionResponse>().await {
        Ok(openai_resp) => {
            let anthropic_resp = translate_to_anthropic(&openai_resp);
            Json(anthropic_resp).into_response()
        }
        Err(e) => {
            error!("解析 chat/completions 响应失败：{}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// 流式：逐行读取 OpenAI SSE，翻译为 Anthropic SSE 事件，写入输出流
async fn messages_stream_response(upstream_resp: reqwest::Response) -> Response {
    // reqwest 字节流 → AsyncRead → 逐行读取
    let byte_stream = upstream_resp.bytes_stream().map(|r| {
        r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });
    let stream_reader = StreamReader::new(byte_stream);
    let mut lines = tokio::io::BufReader::new(stream_reader).lines();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);

    // 后台任务：翻译并写入 channel
    tokio::spawn(async move {
        let mut stream_state = StreamState::new();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    // SSE 数据行格式：data: {...} 或 data: [DONE]
                    let data = match line.strip_prefix("data: ") {
                        Some(d) => d.trim(),
                        None => continue,
                    };

                    if data == "[DONE]" {
                        // MessageStop 已由 translate_chunk 在 finish_reason 时发送，此处只需结束循环
                        break;
                    }

                    let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("解析 SSE chunk 失败：{} | 原始：{}", e, data);
                            continue;
                        }
                    };

                    let events = translate_chunk(&chunk, &mut stream_state);
                    for event in events {
                        let event_type = event_type_name(&event);
                        let json_data = match serde_json::to_string(&event) {
                            Ok(s) => s,
                            Err(e) => {
                                error!("序列化事件失败：{}", e);
                                continue;
                            }
                        };
                        let sse_line = format!("event: {}\ndata: {}\n\n", event_type, json_data);
                        if tx.send(Ok(axum::body::Bytes::from(sse_line))).await.is_err() {
                            // 客户端断开连接，停止发送
                            return;
                        }
                    }
                }
                Ok(None) => {
                    // 上游正常关闭连接，发送终止事件
                    send_event(&tx, StreamEvent::MessageStop).await;
                    break;
                }
                Err(e) => {
                    // 上游 IO 错误，发送错误事件后退出
                    error!("读取上游 SSE 流失败：{}", e);
                    send_event(&tx, StreamEvent::MessageStop).await;
                    break;
                }
            }
        }
    });

    // 将 channel 转换为 axum Body
    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("text/event-stream"));
    headers.insert("cache-control", HeaderValue::from_static("no-cache"));
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    (headers, body).into_response()
}

/// 向 channel 发送一个 SSE 事件，发送失败（客户端断开）时静默忽略
async fn send_event(
    tx: &tokio::sync::mpsc::Sender<Result<axum::body::Bytes, std::io::Error>>,
    event: StreamEvent,
) {
    let event_type = event_type_name(&event);
    if let Ok(json_data) = serde_json::to_string(&event) {
        let sse_line = format!("event: {}\ndata: {}\n\n", event_type, json_data);
        let _ = tx.send(Ok(axum::body::Bytes::from(sse_line))).await;
    }
}

/// 从 StreamEvent 枚举值获取对应的 Anthropic event type 字符串
fn event_type_name(event: &StreamEvent) -> &'static str {
    match event {
        StreamEvent::MessageStart { .. } => "message_start",
        StreamEvent::ContentBlockStart { .. } => "content_block_start",
        StreamEvent::ContentBlockDelta { .. } => "content_block_delta",
        StreamEvent::ContentBlockStop { .. } => "content_block_stop",
        StreamEvent::MessageDelta { .. } => "message_delta",
        StreamEvent::MessageStop => "message_stop",
        StreamEvent::Ping => "ping",
        StreamEvent::Error { .. } => "error",
    }
}


pub async fn serve(state: AppState, host: &str, port: u16) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| anyhow::anyhow!("无效的监听地址 {}:{} — {}", host, port, e))?;
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("服务已启动 → http://{}:{}", host, port);
    axum::serve(listener, router).await?;
    Ok(())
}
