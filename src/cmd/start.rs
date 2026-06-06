use anyhow::Result;
use clap::Args;
use std::sync::Arc;
use tracing::info;

use crate::copilot::models::get_models;
use crate::github::vscode_version::get_vscode_version;
use crate::server::serve;
use crate::state::AppState;
use crate::token::{setup_copilot_token, setup_github_token};

#[derive(Args)]
pub struct StartArgs {
    /// Listening port
    #[arg(short, long, default_value_t = 4142)]
    pub port: u16,

    /// Enable verbose logging (DEBUG level)
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// Provide a GitHub Token directly, skipping Device Flow authorization
    #[arg(short = 'g', long)]
    pub github_token: Option<String>,

    /// Account type: individual / business / enterprise
    #[arg(short, long, default_value = "individual")]
    pub account_type: String,

    /// Display the token in the terminal after successful authorization
    #[arg(long, default_value_t = false)]
    pub show_token: bool,

    /// HTTP/HTTPS proxy address, e.g. http://127.0.0.1:7890
    /// Equivalent to setting both HTTP_PROXY and HTTPS_PROXY env vars
    #[arg(long)]
    pub proxy: Option<String>,

    /// Listening address; defaults to loopback only (127.0.0.1)
    /// Use 0.0.0.0 to listen on all interfaces (accessible on LAN — mind security)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
}

pub async fn run(args: &StartArgs) -> Result<()> {
    // Create a temporary client for the bootstrap phase (state not yet established)
    let bootstrap_client = build_bootstrap_client(args.proxy.as_deref())?;

    // Dynamically fetch the latest VSCode version; it affects the model list returned by GitHub
    let vscode_version = get_vscode_version(&bootstrap_client).await;
    info!("VSCode version: {}", vscode_version);

    // Build global state, passing proxy address explicitly (falls back to env vars when absent)
    let state = AppState::new(&vscode_version, args.proxy.as_deref());
    if let Some(ref proxy) = args.proxy {
        info!("Proxy configured: {}", proxy);
    }

    // Obtain GitHub Token
    if let Some(ref token) = args.github_token {
        info!("Using GitHub Token provided from the command line");
        *state.github_token.write().await = Some(token.clone());
    } else {
        setup_github_token(&state, false).await?;
    }

    if args.show_token {
        if let Some(t) = state.github_token.read().await.as_deref() {
            info!("GitHub Token: {}", t);
        }
    }

    // Exchange for Copilot Token and start background auto-refresh
    setup_copilot_token(state.clone()).await?;

    if args.show_token {
        if let Some(t) = state.copilot_token.read().await.as_deref() {
            info!("Copilot Token: {}", t);
        }
    }

    // Warm up the model list cache; exponential backoff on failure, abort startup if all retries fail
    const MAX_RETRIES: u32 = 3;
    let models = fetch_models_with_retry(&state, MAX_RETRIES).await?;

    info!(
        "🤖 Available models:\n{}",
        models
            .data
            .iter()
            .map(|m| format!("  ✦ {}", m.id))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Warn early if no claude models are present in the list.
    // GitHub Copilot returns different model lists based on the request's source IP:
    // Connections from mainland China IPs have claude-* models filtered out (Anthropic geo-restriction).
    // A non-China proxy reveals the full list where claude-* models appear.
    let claude_count = models
        .data
        .iter()
        .filter(|m| m.id.starts_with("claude-"))
        .count();
    if claude_count == 0 {
        tracing::warn!(
            "No claude-* models were found in the model list; Claude Code / Anthropic requests will not work correctly.\
             This usually means the current egress IP is in mainland China, and the Copilot service filtered Claude models by region.\
             Configure an overseas proxy with --proxy http://127.0.0.1:7890 or HTTP_PROXY/HTTPS_PROXY, then restart."
        );
    }
    // Cache the model ID list as Arc; subsequent requests only clone the Arc, no string copies
    let ids: Vec<String> = models.data.iter().map(|m| m.id.clone()).collect();
    *state.model_ids.write().await = Arc::new(ids);
    *state.models.write().await = Some(models);

    info!("Account type: {}", args.account_type);
    serve(state, &args.host, args.port).await
}

/// Build the temporary HTTP Client used during the bootstrap phase, with optional proxy support
fn build_bootstrap_client(proxy: Option<&str>) -> Result<reqwest::Client> {
    let mut builder = reqwest::ClientBuilder::new();
    if let Some(url) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(url)?);
    }
    Ok(builder.build()?)
}

/// Fetch the model list with exponential backoff retries; return error if all attempts fail
async fn fetch_models_with_retry(
    state: &AppState,
    max_retries: u32,
) -> Result<crate::copilot::models::ModelsResponse> {
    let mut last_err = anyhow::anyhow!("unknown error");
    for attempt in 0..max_retries {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s ...
            let delay = std::time::Duration::from_secs(1 << (attempt - 1));
            tracing::warn!(
                "Failed to fetch model list; retrying in {} seconds (attempt {}/{})",
                delay.as_secs(),
                attempt,
                max_retries - 1
            );
            tokio::time::sleep(delay).await;
        }
        match get_models(&state.client, state).await {
            Ok(models) => return Ok(models),
            Err(e) => last_err = e,
        }
    }
    tracing::error!(
        "Failed to fetch the model list after {} attempts; aborting startup.\
         Check network access to api.githubcopilot.com or configure a proxy with HTTP_PROXY/HTTPS_PROXY.",
        max_retries
    );
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_client_with_proxy() {
        // Valid proxy URL: client should build successfully
        let result = build_bootstrap_client(Some("http://127.0.0.1:7890"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_without_proxy() {
        // No proxy: client should build successfully (falls back to env vars)
        let result = build_bootstrap_client(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_client_invalid_proxy() {
        // reqwest defers proxy URL validation to connection time; no error at build stage is expected
        let result = build_bootstrap_client(Some("not-a-valid-url"));
        assert!(result.is_ok());
    }
}
