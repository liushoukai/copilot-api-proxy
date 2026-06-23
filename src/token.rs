use anyhow::Result;
use tokio::fs;
use tokio::time::{Duration, sleep};
use tracing::{error, info};

use crate::config::paths::{ensure_paths, github_token_path};
use crate::github::access_token::poll_access_token;
use crate::github::copilot_token::get_copilot_token;
use crate::github::device_code::get_device_code;
use crate::github::user::get_github_user;
use crate::state::AppState;

async fn read_github_token() -> Result<String> {
    let content = fs::read_to_string(github_token_path()).await?;
    Ok(content.trim().to_string())
}

async fn write_github_token(token: &str) -> Result<()> {
    fs::write(github_token_path(), token).await?;
    Ok(())
}

/// Persist a GitHub Token provided externally (e.g. from CLI) to the token cache file.
pub async fn persist_github_token(token: &str) -> Result<()> {
    ensure_paths().await?;
    write_github_token(token).await?;
    Ok(())
}

/// Set up GitHub Access Token; all requests use state.client (reads proxy env vars automatically)
pub async fn setup_github_token(state: &AppState, force: bool) -> Result<()> {
    ensure_paths().await?;

    let cached_token = read_github_token().await.unwrap_or_default();
    if !cached_token.is_empty() && !force {
        *state.github_token.write().await = Some(cached_token.clone());
        log_user(state, &cached_token).await?;
        return Ok(());
    }

    info!("Not signed in; starting GitHub Device Flow authorization...");

    let device_code = get_device_code(&state.client).await?;
    info!(
        "Open {} in your browser and enter the authorization code: {}",
        device_code.verification_uri, device_code.user_code
    );

    let token = poll_access_token(&state.client, &device_code).await?;
    write_github_token(&token).await?;
    *state.github_token.write().await = Some(token.clone());

    log_user(state, &token).await?;
    Ok(())
}

/// Proactively refresh the Copilot Token for immediate retry on 401 errors
pub async fn refresh_copilot_token(state: &AppState) -> Result<()> {
    let github_token = state
        .github_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("GitHub Token is not set"))?;

    let vscode_version = state.vscode_version.as_ref();
    let resp = get_copilot_token(&state.client, &github_token, vscode_version).await?;
    *state.copilot_token.write().await = Some(resp.token);
    info!("Copilot Token refreshed proactively");
    Ok(())
}

/// Set up the Copilot Token and start the background periodic refresh task
pub async fn setup_copilot_token(state: AppState) -> Result<()> {
    let github_token = state
        .github_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("GitHub Token is not set"))?;

    let vscode_version = state.vscode_version.as_ref();
    let resp = get_copilot_token(&state.client, &github_token, vscode_version).await?;
    info!(
        "Copilot Token fetched; next refresh in {}s",
        resp.refresh_in
    );
    *state.copilot_token.write().await = Some(resp.token);

    // Background periodic refresh: dynamically adjust next interval using refresh_in after each success.
    // Enforce a minimum 60s gap to prevent busy-loop when refresh_in is abnormally small.
    let mut refresh_interval = calc_refresh_interval(resp.refresh_in);

    tokio::spawn(async move {
        loop {
            sleep(refresh_interval).await;
            info!("Refreshing Copilot Token...");

            let github_token = match state.github_token.read().await.clone() {
                Some(t) => t,
                None => {
                    error!("Failed to refresh Copilot Token: GitHub Token is missing");
                    continue;
                }
            };

            let vscode_version = state.vscode_version.as_ref();
            match get_copilot_token(&state.client, &github_token, vscode_version).await {
                Ok(resp) => {
                    // Update next refresh interval with the new refresh_in
                    refresh_interval = Duration::from_secs(resp.refresh_in.saturating_sub(60));
                    *state.copilot_token.write().await = Some(resp.token);
                    info!(
                        "Copilot Token refreshed; next refresh interval is {}s",
                        refresh_interval.as_secs()
                    );
                }
                Err(e) => error!("Failed to refresh Copilot Token: {}", e),
            }
        }
    });

    Ok(())
}

/// Compute the token refresh interval, guaranteeing at least 60s to prevent busy-loop on abnormal refresh_in
fn calc_refresh_interval(refresh_in: u64) -> Duration {
    let secs = refresh_in.saturating_sub(60).max(60);
    Duration::from_secs(secs)
}

async fn log_user(state: &AppState, github_token: &str) -> Result<()> {
    let user = get_github_user(&state.client, github_token).await?;
    info!("Signed in as: {}", user.login);
    Ok(())
}
