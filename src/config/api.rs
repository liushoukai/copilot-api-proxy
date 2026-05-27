/// GitHub OAuth App Client ID (reused from the VS Code Copilot extension App ID)
pub const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// Requested permission scope; read:user is sufficient to exchange for a Copilot Token
pub const GITHUB_APP_SCOPES: &str = "read:user";

/// GitHub web URL (Device Flow authorization page base)
pub const GITHUB_BASE_URL: &str = "https://github.com";

/// GitHub REST API base URL
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// Emulated VS Code Copilot plugin version, used to pass API legitimacy checks
pub const COPILOT_VERSION: &str = "0.26.7";

pub fn user_agent() -> String {
    format!("GitHubCopilotChat/{}", COPILOT_VERSION)
}

pub fn editor_plugin_version() -> String {
    format!("copilot-chat/{}", COPILOT_VERSION)
}
