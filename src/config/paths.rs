use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;

/// Application config directory: ~/.config/copilot-api-proxy
fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("copilot-api-proxy")
}

/// Path to the cached GitHub Token file
pub fn github_token_path() -> PathBuf {
    app_dir().join("github_token")
}

/// Ensure the required directories and files exist
pub async fn ensure_paths() -> Result<()> {
    let dir = app_dir();
    fs::create_dir_all(&dir).await?;
    ensure_file(github_token_path()).await?;
    Ok(())
}

/// Create an empty file if it does not exist and set permissions to 0600
async fn ensure_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        fs::write(&path, "").await?;

        // Set file permissions on Unix only
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms)?;
        }
    }
    Ok(())
}
