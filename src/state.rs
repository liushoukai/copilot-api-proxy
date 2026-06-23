use std::sync::Arc;
use tokio::sync::RwLock;

use crate::copilot::models::ModelsResponse;

/// Global application state; Arc + RwLock allows concurrent access across async tasks.
#[derive(Clone)]
pub struct AppState {
    /// Shared HTTP client with optional explicit proxy configuration
    pub client: reqwest::Client,
    /// Long-lived GitHub Access Token, sourced from file cache or Device Flow
    pub github_token: Arc<RwLock<Option<String>>>,
    /// Short-lived Copilot Token (~30 min), refreshed automatically in the background
    pub copilot_token: Arc<RwLock<Option<String>>>,
    /// Available model list cached at startup
    pub models: Arc<RwLock<Option<ModelsResponse>>>,
    /// Cached model ID list (inner Arc: cloning only bumps the ref-count, no string copies)
    pub model_ids: Arc<RwLock<Arc<Vec<String>>>>,
    /// Emulated VSCode version; immutable after startup, stored in Arc to avoid lock overhead
    pub vscode_version: Arc<String>,
    /// Max messages to forward upstream; oldest dropped when exceeded
    pub max_messages: Option<usize>,
}

impl AppState {
    /// Create application state. proxy is an optional proxy address (e.g. http://127.0.0.1:7890);
    /// when omitted, HTTP_PROXY / HTTPS_PROXY env vars are read automatically.
    pub fn new(vscode_version: &str, proxy: Option<&str>, max_messages: Option<usize>) -> Self {
        let client = build_client(proxy).expect("failed to build HTTP client");
        Self {
            client,
            github_token: Arc::new(RwLock::new(None)),
            copilot_token: Arc::new(RwLock::new(None)),
            models: Arc::new(RwLock::new(None)),
            model_ids: Arc::new(RwLock::new(Arc::new(Vec::new()))),
            vscode_version: Arc::new(vscode_version.to_string()),
            max_messages,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_appstate_stores_max_messages() {
        let state = AppState::new("1.99.0", None, Some(20));
        assert_eq!(state.max_messages, Some(20));
    }

    #[test]
    fn test_appstate_no_max_messages() {
        let state = AppState::new("1.99.0", None, None);
        assert_eq!(state.max_messages, None);
    }
}

/// Build a reqwest Client. An explicit proxy takes priority; otherwise falls back to env vars.
fn build_client(proxy: Option<&str>) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::ClientBuilder::new();
    if let Some(url) = proxy {
        // Explicit proxy: applies to both HTTP and HTTPS
        builder = builder.proxy(reqwest::Proxy::all(url)?);
    }
    // When proxy is absent, reqwest still reads HTTP_PROXY / HTTPS_PROXY by default
    Ok(builder.build()?)
}
