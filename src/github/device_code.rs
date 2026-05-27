use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::api::{GITHUB_APP_SCOPES, GITHUB_BASE_URL, GITHUB_CLIENT_ID};

/// Device code information returned by the first step of GitHub Device Flow
#[derive(Debug, Deserialize, Serialize)]
pub struct DeviceCodeResponse {
    /// Passed to the polling endpoint; not displayed to the user
    pub device_code: String,
    /// Shown to the user for manual entry in the browser
    pub user_code: String,
    /// Authorization page the user must visit
    pub verification_uri: String,
    /// Lifetime of the device code in seconds
    pub expires_in: u64,
    /// Polling interval in seconds to avoid GitHub rate limits
    pub interval: u64,
}

/// Request a device code from GitHub to initiate the Device Flow authorization process
pub async fn get_device_code(client: &reqwest::Client) -> Result<DeviceCodeResponse> {
    let url = format!("{}/login/device/code", GITHUB_BASE_URL);

    let body = serde_json::json!({
        "client_id": GITHUB_CLIENT_ID,
        "scope": GITHUB_APP_SCOPES,
    });

    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(&body)
        .send()
        .await
        .context("failed to request device code")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to fetch device code: {}", text);
    }

    resp.json::<DeviceCodeResponse>()
        .await
        .context("failed to parse device code response")
}
