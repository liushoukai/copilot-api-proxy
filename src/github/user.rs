use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::api::{GITHUB_API_BASE_URL, user_agent};

#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    /// GitHub 用户名
    pub login: String,
}

/// 获取当前已认证的 GitHub 用户信息，用于登录成功后打印用户名
pub async fn get_github_user(
    client: &reqwest::Client,
    github_token: &str,
) -> Result<GitHubUser> {
    let url = format!("{}/user", GITHUB_API_BASE_URL);

    let resp = client
        .get(&url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("authorization", format!("token {}", github_token))
        .header("user-agent", user_agent())
        .send()
        .await
        .context("请求用户信息失败")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("获取用户信息失败：{}", text);
    }

    resp.json::<GitHubUser>()
        .await
        .context("解析用户信息响应失败")
}
