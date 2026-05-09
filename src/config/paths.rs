use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

/// 应用数据目录：~/.local/share/copilot-api-proxy
fn app_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("copilot-api-proxy")
}

/// GitHub Token 缓存文件路径
pub fn github_token_path() -> PathBuf {
    app_dir().join("github_token")
}

/// 确保必要目录和文件存在
pub async fn ensure_paths() -> Result<()> {
    let dir = app_dir();
    fs::create_dir_all(&dir).await?;
    ensure_file(github_token_path()).await?;
    Ok(())
}

/// 如果文件不存在则创建空文件，并设置权限 0600
async fn ensure_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        fs::write(&path, "").await?;

        // 仅 Unix 系统设置文件权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }
    }
    Ok(())
}
