use anyhow::Result;
use serde::Deserialize;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, error, info};

use crate::config::api::{GITHUB_BASE_URL, GITHUB_CLIENT_ID};
use crate::github::device_code::DeviceCodeResponse;

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    /// access_token returned on successful authorization
    access_token: Option<String>,
    /// Error code when authorization is still pending, e.g. authorization_pending
    error: Option<String>,
}

/// Poll GitHub until the user completes authorization in the browser, then return the access_token
pub async fn poll_access_token(
    client: &reqwest::Client,
    device_code: &DeviceCodeResponse,
) -> Result<String> {
    // Add 1s on top of interval to avoid hitting GitHub rate limits
    let sleep_duration = Duration::from_secs(device_code.interval + 1);
    // Deadline for the device code; stop polling after this point
    let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);
    debug!(
        "Polling access_token every {}ms; expires in {}s",
        sleep_duration.as_millis(),
        device_code.expires_in
    );

    let url = format!("{}/login/oauth/access_token", GITHUB_BASE_URL);
    let body = serde_json::json!({
        "client_id": GITHUB_CLIENT_ID,
        "device_code": device_code.device_code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    });

    loop {
        // Stop polling once the device code has expired
        if Instant::now() >= deadline {
            anyhow::bail!(
                "Device Flow authorization timed out (not completed within {} seconds); rerun the auth command",
                device_code.expires_in
            );
        }

        let resp = client
            .post(&url)
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .json(&body)
            .send()
            .await;

        // Wait and retry on network error
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                error!("Polling request failed: {}", e);
                sleep(sleep_duration).await;
                continue;
            }
        };

        if !resp.status().is_success() {
            error!("Polling response was not 2xx: {}", resp.status());
            sleep(sleep_duration).await;
            continue;
        }

        let token_resp: AccessTokenResponse = match resp.json().await {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse polling response: {}", e);
                sleep(sleep_duration).await;
                continue;
            }
        };

        debug!("Polling response: error={:?}", token_resp.error);

        // Authorization successful once access_token is received
        if let Some(token) = token_resp.access_token {
            if !token.is_empty() {
                return Ok(token);
            }
        }

        // Device code expired; abort immediately without further waiting
        if token_resp.error.as_deref() == Some("expired_token") {
            anyhow::bail!("Authorization code expired; rerun the auth command");
        }

        // slow_down: GitHub requests a lower polling rate; wait an extra interval
        if token_resp.error.as_deref() == Some("slow_down") {
            info!(
                "GitHub requested a lower polling frequency; waiting an extra {}s",
                sleep_duration.as_secs()
            );
            sleep(sleep_duration * 2).await;
            continue;
        }

        // authorization_pending means the user hasn't finished yet; keep waiting
        sleep(sleep_duration).await;
    }
}
