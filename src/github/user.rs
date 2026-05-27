use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::api::{GITHUB_API_BASE_URL, user_agent};

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    /// GitHub username
    pub login: String,
}

/// Fetch the currently authenticated GitHub user, used to log the username after sign-in
pub async fn get_github_user(client: &reqwest::Client, github_token: &str) -> Result<GitHubUser> {
    let url = format!("{}/user", GITHUB_API_BASE_URL);

    let resp = client
        .get(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("authorization", format!("token {}", github_token))
        .header("user-agent", user_agent())
        .send()
        .await
        .context("failed to request user information")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("failed to fetch user information: {}", text);
    }

    resp.json::<GitHubUser>()
        .await
        .context("failed to parse user information response")
}
