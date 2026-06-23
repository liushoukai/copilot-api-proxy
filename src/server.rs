use std::net::SocketAddr;

use anyhow::Result;
use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use futures::StreamExt;
use humansize::{DECIMAL, format_size};
use serde_json::{Value, json};
use tokio::io::AsyncBufReadExt;
use tokio_util::io::StreamReader;
use tracing::{error, info};

use crate::anthropic::stream::translate_chunk;
use crate::anthropic::translate::{translate_to_anthropic, translate_to_openai};
use crate::anthropic::types::{MessagesPayload, StreamEvent, StreamState};
use crate::copilot::chat::{
    ChatCompletionChunk, ChatCompletionResponse, ChatCompletionsPayload, create_chat_completions,
    to_sse_body,
};
use crate::copilot::embeddings::{EmbeddingRequest, create_embeddings};
use crate::copilot::models::get_models;
use crate::state::AppState;
use crate::token::refresh_copilot_token;

/// Middleware that logs each request path and Content-Length without buffering the body.
async fn log_request_size_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let size = req
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| format_size(n, DECIMAL))
        .unwrap_or_else(|| "-".to_string());
    info!("{} {} request body size: {}", method, path, size);
    next.run(req).await
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/models", get(models_handler))
        .route("/chat/completions", post(chat_completions_handler))
        .route("/embeddings", post(embeddings_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/chat/completions", post(chat_completions_handler))
        .route("/v1/embeddings", post(embeddings_handler))
        .route("/v1/messages", post(messages_handler))
        .layer(from_fn(log_request_size_middleware))
        .with_state(state.clone())
}

async fn health_handler(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "github_token": state.github_token.read().await.is_some(),
        "copilot_token": state.copilot_token.read().await.is_some(),
    }))
}

async fn models_handler(State(state): State<AppState>) -> Response {
    // Prefer the model list cached at startup.
    {
        let cached = state.models.read().await;
        if let Some(ref models_resp) = *cached {
            let data: Vec<Value> = models_resp.data.iter().map(model_to_json).collect();
            return Json(json!({ "object": "list", "data": data })).into_response();
        }
    }

    // Fall back to a live request when the cache is empty.
    match get_models(&state.client, &state).await {
        Ok(resp) => {
            let data: Vec<Value> = resp.data.iter().map(model_to_json).collect();
            Json(json!({ "object": "list", "data": data })).into_response()
        }
        Err(e) => {
            error!("Failed to fetch model list: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn chat_completions_handler(
    State(state): State<AppState>,
    Json(mut payload): Json<ChatCompletionsPayload>,
) -> Response {
    let is_stream = payload.stream.unwrap_or(false);

    // Read model IDs from cache. Arc clone avoids string copies.
    let available_models = state.model_ids.read().await.clone();
    let original_model = payload.model.clone();
    payload.model = crate::anthropic::translate::resolve_model(&payload.model, &available_models);
    if payload.model != original_model {
        info!(
            "chat/completions → model: {} → {}",
            original_model, payload.model
        );
    }

    let upstream_resp = match create_with_retry(&state, &payload, "chat/completions").await {
        Ok(r) => r,
        Err(e) => return e,
    };

    if is_stream {
        // Streaming mode: pass the Copilot SSE byte stream through to the client.
        return (sse_headers(), to_sse_body(upstream_resp)).into_response();
    }

    match upstream_resp.json::<Value>().await {
        Ok(json) => Json(json).into_response(),
        Err(e) => {
            error!("Failed to parse chat/completions response: {}", e);
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
            error!("Embeddings request failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// Treat both 400 (invalid token format) and 401 (expired token) as auth errors for proactive refresh.
fn is_auth_error(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST
}

/// Send an upstream request; refresh the token and retry once on authentication failure.
/// Also retries once on 408 (server-side body read timeout) without token refresh.
async fn create_with_retry(
    state: &AppState,
    payload: &ChatCompletionsPayload,
    tag: &str,
) -> Result<reqwest::Response, Response> {
    let mut resp = match create_chat_completions(&state.client, state, payload).await {
        Ok(r) => r,
        Err(e) => {
            error!("{} request failed: {}", tag, e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response());
        }
    };

    if is_auth_error(resp.status()) {
        info!(
            "Copilot Token authentication failed ({}); refreshing and retrying...",
            resp.status()
        );
        if let Err(e) = refresh_copilot_token(state).await {
            error!("Failed to refresh Copilot Token: {}", e);
            return Err((
                StatusCode::UNAUTHORIZED,
                resp.text().await.unwrap_or_default(),
            )
                .into_response());
        }
        resp = match create_chat_completions(&state.client, state, payload).await {
            Ok(r) => r,
            Err(e) => {
                error!("{} retry request failed: {}", tag, e);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response());
            }
        };
    }

    if resp.status() == StatusCode::REQUEST_TIMEOUT {
        tracing::warn!(
            "{} got 408 (server body read timeout, large request body); retrying once...",
            tag
        );
        resp = match create_chat_completions(&state.client, state, payload).await {
            Ok(r) => r,
            Err(e) => {
                error!("{} retry after 408 failed: {}", tag, e);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response());
            }
        };
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        error!("Copilot {} returned error {}: {}", tag, status, body);
        return Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            body,
        )
            .into_response());
    }

    Ok(resp)
}

/// Build SSE streaming response headers.
fn sse_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert("cache-control", HeaderValue::from_static("no-cache"));
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    headers
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

/// Anthropic Messages API handler; translates requests to Copilot and responses back to Anthropic.
async fn messages_handler(
    State(state): State<AppState>,
    Json(mut payload): Json<MessagesPayload>,
) -> Response {
    let is_stream = payload.stream.unwrap_or(false);

    // Read model IDs from cache. Arc clone avoids string copies.
    let available_models = state.model_ids.read().await.clone();

    // Strip x-anthropic-billing-header entries from the system array before forwarding.
    if let Some(Value::Array(arr)) = payload.system.as_mut() {
        let before = arr.len();
        arr.retain(|item| {
            let text = item.get("text").and_then(Value::as_str).unwrap_or("");
            if text.starts_with("x-anthropic-billing-header") {
                info!("x-anthropic-billing-header: {}", text);
                return false;
            }
            true
        });
        let removed = before - arr.len();
        if removed > 0 {
            info!(
                "stripped {} x-anthropic-billing-header item(s) from system array",
                removed
            );
        }
    }

    let openai_payload = translate_to_openai(&payload, &available_models);
    let forwarded_size = serde_json::to_vec(&openai_payload)
        .map(|b| format_size(b.len() as u64, DECIMAL))
        .unwrap_or_else(|_| "-".to_string());
    info!(
        "messages → model: {} → {}; forwarded body size: {}",
        payload.model, openai_payload.model, forwarded_size
    );

    let upstream_resp = match create_with_retry(&state, &openai_payload, "messages").await {
        Ok(r) => r,
        Err(e) => return e,
    };

    if is_stream {
        return messages_stream_response(upstream_resp).await;
    }

    messages_non_stream_response(upstream_resp).await
}

/// Non-streaming mode: parse the OpenAI response and convert it to Anthropic MessagesResponse.
async fn messages_non_stream_response(upstream_resp: reqwest::Response) -> Response {
    match upstream_resp.json::<ChatCompletionResponse>().await {
        Ok(openai_resp) => {
            let anthropic_resp = translate_to_anthropic(&openai_resp);
            Json(anthropic_resp).into_response()
        }
        Err(e) => {
            error!("Failed to parse chat/completions response: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// Streaming mode: read OpenAI SSE lines, translate them to Anthropic SSE events, and write to the output stream.
async fn messages_stream_response(upstream_resp: reqwest::Response) -> Response {
    // Convert the reqwest byte stream to AsyncRead, then read it line by line.
    let byte_stream = upstream_resp
        .bytes_stream()
        .map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
    let stream_reader = StreamReader::new(byte_stream);
    let mut lines = tokio::io::BufReader::new(stream_reader).lines();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, std::io::Error>>(64);

    // Background task: translate events and write them into the channel.
    tokio::spawn(async move {
        let mut stream_state = StreamState::new();
        // Reuse allocation across chunks to avoid per-chunk heap allocation.
        let mut events: Vec<StreamEvent> = Vec::new();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    // SSE data line format: data: {...} or data: [DONE]
                    let data = match line.strip_prefix("data: ") {
                        Some(d) => d.trim(),
                        None => continue,
                    };

                    if data == "[DONE]" {
                        // MessageStop is emitted by translate_chunk on finish_reason; only stop the loop here.
                        break;
                    }

                    let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to parse SSE chunk: {} | raw: {}", e, data);
                            continue;
                        }
                    };

                    events.clear();
                    translate_chunk(&chunk, &mut stream_state, &mut events);
                    for event in &events {
                        let event_type = event_type_name(event);
                        let json_data = match serde_json::to_string(event) {
                            Ok(s) => s,
                            Err(e) => {
                                error!("Failed to serialize event: {}", e);
                                continue;
                            }
                        };
                        let sse_line = format!("event: {}\ndata: {}\n\n", event_type, json_data);
                        if tx
                            .send(Ok(axum::body::Bytes::from(sse_line)))
                            .await
                            .is_err()
                        {
                            // Client disconnected; stop sending.
                            return;
                        }
                    }
                }
                Ok(None) => {
                    // Upstream closed normally; send the terminal event.
                    send_event(&tx, StreamEvent::MessageStop).await;
                    break;
                }
                Err(e) => {
                    // Upstream IO error; send the terminal event and exit.
                    error!("Failed to read upstream SSE stream: {}", e);
                    send_event(&tx, StreamEvent::MessageStop).await;
                    break;
                }
            }
        }
    });

    // Convert the channel into an axum Body.
    let body = Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx));
    (sse_headers(), body).into_response()
}

/// Send an SSE event to the channel; silently ignore send failures caused by client disconnects.
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

/// Get the Anthropic event type string for a StreamEvent variant.
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
        .map_err(|e| anyhow::anyhow!("Invalid listen address {}:{}: {}", host, port, e))?;
    let router = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Server started at http://{}:{}", host, port);
    axum::serve(listener, router).await?;
    Ok(())
}
