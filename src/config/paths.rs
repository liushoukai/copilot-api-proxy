use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Application config directory: ~/.config/copilot-api-proxy
fn app_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    app_dir_with_home(&home)
}

fn app_dir_with_home(home: &Path) -> PathBuf {
    home.join(".config").join("copilot-api-proxy")
}

/// Path to the cached GitHub Token file
pub fn github_token_path() -> PathBuf {
    app_dir().join("github_token")
}

/// Path to the proxy configuration file
pub fn config_file_path() -> PathBuf {
    app_dir().join("config.toml")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_token_path_uses_home_config_directory() {
        let temp_dir = tempfile::tempdir().expect("create temp home");

        let expected = temp_dir
            .path()
            .join(".config")
            .join("copilot-api-proxy")
            .join("github_token");

        assert_eq!(
            app_dir_with_home(temp_dir.path()).join("github_token"),
            expected
        );
    }
}
