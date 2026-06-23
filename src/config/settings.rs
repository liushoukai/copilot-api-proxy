use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct ProxySettings {
    #[serde(default)]
    pub proxy: ProxySection,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProxySection {
    pub max_messages: Option<usize>,
}

/// Load proxy settings from config.toml. Missing or unparseable file returns defaults silently.
pub async fn load(path: &Path) -> ProxySettings {
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return ProxySettings::default();
    };
    toml::from_str(&content).unwrap_or_else(|e| {
        tracing::warn!("Failed to parse config.toml: {}; using defaults", e);
        ProxySettings::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_max_messages() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "[proxy]\nmax_messages = 30").unwrap();
        let settings = load(f.path()).await;
        assert_eq!(settings.proxy.max_messages, Some(30));
    }

    #[tokio::test]
    async fn test_load_missing_file_returns_default() {
        let settings = load(Path::new("/nonexistent/config.toml")).await;
        assert_eq!(settings.proxy.max_messages, None);
    }

    #[tokio::test]
    async fn test_load_empty_file_returns_default() {
        let f = NamedTempFile::new().unwrap();
        let settings = load(f.path()).await;
        assert_eq!(settings.proxy.max_messages, None);
    }
}
