use tracing::warn;

/// VSCode 版本兜底值
const FALLBACK_VERSION: &str = "1.117.0";

/// 从 AUR PKGBUILD 动态获取最新 VSCode 版本号
///
/// 获取失败时返回兜底版本，不影响启动流程
pub async fn get_vscode_version(client: &reqwest::Client) -> String {
    let result = fetch_version(client).await;
    result.unwrap_or_else(|e| {
        warn!("获取 VSCode 版本失败，使用兜底版本 {}：{}", FALLBACK_VERSION, e);
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

    // 从 PKGBUILD 中提取 pkgver=x.x.x
    let version = text
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("pkgver=")
        })
        .ok_or_else(|| anyhow::anyhow!("未找到 pkgver 字段"))?
        .to_string();

    Ok(version)
}
