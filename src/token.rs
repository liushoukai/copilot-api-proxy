use anyhow::Result;
use tokio::fs;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info};

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

/// 设置 GitHub Access Token，所有请求统一走 state.client（自动读代理环境变量）
pub async fn setup_github_token(state: &AppState, force: bool) -> Result<()> {
    ensure_paths().await?;

    let cached_token = read_github_token().await.unwrap_or_default();
    if !cached_token.is_empty() && !force {
        *state.github_token.write().await = Some(cached_token.clone());
        log_user(state, &cached_token).await?;
        return Ok(());
    }

    info!("未登录，开始 GitHub Device Flow 授权...");

    let device_code = get_device_code(&state.client).await?;
    info!(
        "请在浏览器中打开 {} 并输入授权码：{}",
        device_code.verification_uri, device_code.user_code
    );

    let token = poll_access_token(&state.client, &device_code).await?;
    write_github_token(&token).await?;
    *state.github_token.write().await = Some(token.clone());

    log_user(state, &token).await?;
    Ok(())
}

/// 主动刷新 Copilot Token，用于 401 错误时立即重试
pub async fn refresh_copilot_token(state: &AppState) -> Result<()> {
    let github_token = state
        .github_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("GitHub Token 未设置"))?;

    let vscode_version = state.vscode_version.as_ref();
    let resp = get_copilot_token(&state.client, &github_token, vscode_version).await?;
    *state.copilot_token.write().await = Some(resp.token);
    info!("Copilot Token 主动刷新成功");
    Ok(())
}

/// 设置 Copilot Token，并启动后台定时刷新任务
pub async fn setup_copilot_token(state: AppState) -> Result<()> {
    let github_token = state
        .github_token
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("GitHub Token 未设置"))?;

    let vscode_version = state.vscode_version.as_ref();
    let resp = get_copilot_token(&state.client, &github_token, vscode_version).await?;
    debug!("Copilot Token 获取成功，将在 {}s 后刷新", resp.refresh_in);
    *state.copilot_token.write().await = Some(resp.token);

    // 后台定时刷新，每次成功后用最新 refresh_in 动态调整下次间隔
    // 至少保留 60 秒间隔，防止 refresh_in 异常过小时导致 busy-loop
    let mut refresh_interval = calc_refresh_interval(resp.refresh_in);

    tokio::spawn(async move {
        loop {
            sleep(refresh_interval).await;
            debug!("开始刷新 Copilot Token...");

            let github_token = match state.github_token.read().await.clone() {
                Some(t) => t,
                None => {
                    error!("刷新 Copilot Token 失败：GitHub Token 不存在");
                    continue;
                }
            };

            let vscode_version = state.vscode_version.as_ref();
            match get_copilot_token(&state.client, &github_token, vscode_version).await {
                Ok(resp) => {
                    // 用新的 refresh_in 更新下次刷新间隔
                    refresh_interval = Duration::from_secs(resp.refresh_in.saturating_sub(60));
                    *state.copilot_token.write().await = Some(resp.token);
                    debug!("Copilot Token 刷新成功，下次刷新间隔 {}s", refresh_interval.as_secs());
                }
                Err(e) => error!("Copilot Token 刷新失败：{}", e),
            }
        }
    });

    Ok(())
}

/// 计算 Token 刷新间隔，确保至少 60 秒，防止 refresh_in 异常时 busy-loop
fn calc_refresh_interval(refresh_in: u64) -> Duration {
    let secs = refresh_in.saturating_sub(60).max(60);
    Duration::from_secs(secs)
}

async fn log_user(state: &AppState, github_token: &str) -> Result<()> {
    let user = get_github_user(&state.client, github_token).await?;
    info!("已登录为：{}", user.login);
    Ok(())
}
