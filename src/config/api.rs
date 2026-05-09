/// GitHub OAuth App Client ID（复用 VS Code Copilot 插件的 App ID）
pub const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// 申请的权限范围，read:user 足够换取 Copilot Token
pub const GITHUB_APP_SCOPES: &str = "read:user";

/// GitHub 网页端地址（Device Flow 授权页面）
pub const GITHUB_BASE_URL: &str = "https://github.com";

/// GitHub REST API 地址
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// 模拟 VS Code Copilot 插件版本，通过 API 合法性校验
pub const COPILOT_VERSION: &str = "0.26.7";

pub fn user_agent() -> String {
    format!("GitHubCopilotChat/{}", COPILOT_VERSION)
}

pub fn editor_plugin_version() -> String {
    format!("copilot-chat/{}", COPILOT_VERSION)
}
