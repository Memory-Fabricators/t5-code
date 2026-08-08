//! Port of `src/constants/oauth.ts`.

use crate::utils::env_utils::is_env_var_truthy;

fn get_oauth_config_type() -> &'static str {
    if std::env::var("USER_TYPE").ok().as_deref() == Some("ant") {
        if is_env_var_truthy("USE_LOCAL_OAUTH") {
            return "local";
        }
        if is_env_var_truthy("USE_STAGING_OAUTH") {
            return "staging";
        }
    }
    "prod"
}

/// Filename suffix for the global config JSON (empty for production).
pub fn file_suffix_for_oauth_config() -> &'static str {
    if std::env::var_os("CLAUDE_CODE_CUSTOM_OAUTH_URL").is_some() {
        return "-custom-oauth";
    }
    match get_oauth_config_type() {
        "local" => "-local-oauth",
        "staging" => "-staging-oauth",
        _ => "",
    }
}

pub const CLAUDE_AI_INFERENCE_SCOPE: &str = "user:inference";
pub const CLAUDE_AI_PROFILE_SCOPE: &str = "user:profile";
pub const CONSOLE_SCOPE: &str = "org:create_api_key";
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

pub const CONSOLE_OAUTH_SCOPES: &[&str] = &[CONSOLE_SCOPE, CLAUDE_AI_PROFILE_SCOPE];

pub const CLAUDE_AI_OAUTH_SCOPES: &[&str] = &[
    CLAUDE_AI_PROFILE_SCOPE,
    CLAUDE_AI_INFERENCE_SCOPE,
    "user:sessions:claude_code",
    "user:mcp_servers",
    "user:file_upload",
];

/// Union of Console + Claude.ai scopes requested at login.
pub const ALL_OAUTH_SCOPES: &[&str] = &[
    CONSOLE_SCOPE,
    CLAUDE_AI_PROFILE_SCOPE,
    CLAUDE_AI_INFERENCE_SCOPE,
    "user:sessions:claude_code",
    "user:mcp_servers",
    "user:file_upload",
];

/// Production OAuth endpoints / client id (staging/local deferred).
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub const BASE_API_URL: &str = "https://api.anthropic.com";
pub const CLAUDE_AI_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
pub const CONSOLE_AUTHORIZE_URL: &str = "https://platform.claude.com/oauth/authorize";
pub const API_KEY_URL: &str = "https://api.anthropic.com/api/oauth/claude_cli/create_api_key";
pub const ROLES_URL: &str = "https://api.anthropic.com/api/oauth/claude_cli/roles";
pub const CONSOLE_SUCCESS_URL: &str =
    "https://platform.claude.com/buy_credits?returnUrl=/oauth/code/success%3Fapp%3Dclaude-code";
pub const CLAUDEAI_SUCCESS_URL: &str =
    "https://platform.claude.com/oauth/code/success?app=claude-code";
pub const MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";

/// Whether scopes indicate a Claude.ai subscriber auth (has inference scope).
pub fn should_use_claude_ai_auth(scopes: Option<&[String]>) -> bool {
    scopes
        .map(|s| s.iter().any(|scope| scope == CLAUDE_AI_INFERENCE_SCOPE))
        .unwrap_or(false)
}
