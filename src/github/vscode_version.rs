use tracing::warn;

/// Fallback VSCode version
const FALLBACK_VERSION: &str = "1.117.0";

/// Dynamically fetch the latest VSCode version from the AUR PKGBUILD.
///
/// Returns the fallback version on failure so the startup process is unaffected.
pub async fn get_vscode_version(client: &reqwest::Client) -> String {
    let result = fetch_version(client).await;
    result.unwrap_or_else(|e| {
        warn!(
            "Failed to fetch VSCode version; using fallback version {}: {}",
            FALLBACK_VERSION, e
        );
        FALLBACK_VERSION.to_string()
    })
}

async fn fetch_version(client: &reqwest::Client) -> anyhow::Result<String> {
    let resp = client
        .get("https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=visual-studio-code-bin")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    let text = resp.text().await?;

    // Extract pkgver=x.x.x from the PKGBUILD
    let version = text
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("pkgver=")
        })
        .ok_or_else(|| anyhow::anyhow!("pkgver field not found"))?
        .to_string();

    Ok(version)
}
