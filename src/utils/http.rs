//! Port of `src/utils/http.ts` — auth headers + user agent for Anthropic API.

use crate::constants::oauth::OAUTH_BETA_HEADER;
use crate::utils::auth::{
    get_claude_ai_oauth_tokens, has_anthropic_api_key_env, is_claude_ai_subscriber,
};

pub struct AuthHeaders {
    pub headers: Vec<(String, String)>,
    pub error: Option<String>,
}

pub fn get_user_agent() -> String {
    let user_type = std::env::var("USER_TYPE").unwrap_or_else(|_| "external".into());
    let entrypoint = std::env::var("CLAUDE_CODE_ENTRYPOINT").unwrap_or_else(|_| "cli".into());
    format!(
        "claude-cli/{} ({user_type}, {entrypoint})",
        env!("CARGO_PKG_VERSION")
    )
}

/// OAuth Bearer + beta, or x-api-key from ANTHROPIC_API_KEY / primaryApiKey.
pub fn get_auth_headers() -> AuthHeaders {
    if is_claude_ai_subscriber() {
        match get_claude_ai_oauth_tokens() {
            Some(tokens) if !tokens.access_token.is_empty() => AuthHeaders {
                headers: vec![
                    (
                        "Authorization".into(),
                        format!("Bearer {}", tokens.access_token),
                    ),
                    ("anthropic-beta".into(), OAUTH_BETA_HEADER.into()),
                ],
                error: None,
            },
            _ => AuthHeaders {
                headers: vec![],
                error: Some("No OAuth token available".into()),
            },
        }
    } else if has_anthropic_api_key_env() {
        let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        AuthHeaders {
            headers: vec![("x-api-key".into(), key)],
            error: None,
        }
    } else if let Some(key) = crate::utils::auth::get_primary_api_key() {
        AuthHeaders {
            headers: vec![("x-api-key".into(), key)],
            error: None,
        }
    } else {
        AuthHeaders {
            headers: vec![],
            error: Some("No API key available".into()),
        }
    }
}
