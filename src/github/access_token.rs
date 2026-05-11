use anyhow::Result;
use serde::Deserialize;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, error, info};

use crate::config::api::{GITHUB_BASE_URL, GITHUB_CLIENT_ID};
use crate::github::device_code::DeviceCodeResponse;

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    /// 授权成功时返回的 access_token
    access_token: Option<String>,
    /// 授权未完成时返回的错误码，如 authorization_pending
    error: Option<String>,
}

/// 轮询 GitHub，等待用户在浏览器完成授权，返回 access_token
pub async fn poll_access_token(
    client: &reqwest::Client,
    device_code: &DeviceCodeResponse,
) -> Result<String> {
    // 在 interval 基础上额外加 1 秒，避免触发 GitHub 限流
    let sleep_duration = Duration::from_secs(device_code.interval + 1);
    // 授权码截止时间，超时后不再轮询
    let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);
    debug!("轮询 access_token，间隔 {}ms，有效期 {}s", sleep_duration.as_millis(), device_code.expires_in);

    let url = format!("{}/login/oauth/access_token", GITHUB_BASE_URL);
    let body = serde_json::json!({
        "client_id": GITHUB_CLIENT_ID,
        "device_code": device_code.device_code,
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
    });

    loop {
        // 超过授权码有效期则终止轮询
        if Instant::now() >= deadline {
            anyhow::bail!(
                "Device Flow 授权超时（{}秒内未完成），请重新运行 auth 命令",
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

        // 网络错误时等待后重试
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                error!("轮询请求失败：{}", e);
                sleep(sleep_duration).await;
                continue;
            }
        };

        if !resp.status().is_success() {
            error!("轮询响应非 2xx：{}", resp.status());
            sleep(sleep_duration).await;
            continue;
        }

        let token_resp: AccessTokenResponse = match resp.json().await {
            Ok(r) => r,
            Err(e) => {
                error!("解析轮询响应失败：{}", e);
                sleep(sleep_duration).await;
                continue;
            }
        };

        debug!("轮询响应：error={:?}", token_resp.error);

        // 拿到 access_token 则授权成功
        if let Some(token) = token_resp.access_token {
            if !token.is_empty() {
                return Ok(token);
            }
        }

        // 授权码已过期，立即终止，无需继续等待
        if token_resp.error.as_deref() == Some("expired_token") {
            anyhow::bail!("授权码已过期，请重新运行 auth 命令");
        }

        // slow_down：GitHub 要求降低轮询频率，额外多等一个间隔
        if token_resp.error.as_deref() == Some("slow_down") {
            info!("GitHub 要求降低轮询频率，额外等待 {}s", sleep_duration.as_secs());
            sleep(sleep_duration * 2).await;
            continue;
        }

        // authorization_pending 表示用户尚未完成授权，继续等待
        sleep(sleep_duration).await;
    }
}
