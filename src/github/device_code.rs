use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::api::{GITHUB_APP_SCOPES, GITHUB_BASE_URL, GITHUB_CLIENT_ID};

/// GitHub Device Flow 第一步返回的设备码信息
#[derive(Debug, Deserialize, Serialize)]
pub struct DeviceCodeResponse {
    /// 传给轮询接口使用，不展示给用户
    pub device_code: String,
    /// 展示给用户，需在浏览器中手动输入
    pub user_code: String,
    /// 用户需要访问的授权页面
    pub verification_uri: String,
    /// 设备码的有效时长（秒）
    pub expires_in: u64,
    /// 轮询间隔（秒），避免触发 GitHub 限流
    pub interval: u64,
}

/// 向 GitHub 申请设备码，开启 Device Flow 授权流程
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
        .context("请求设备码失败")?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("获取设备码失败：{}", text);
    }

    resp.json::<DeviceCodeResponse>()
        .await
        .context("解析设备码响应失败")
}
