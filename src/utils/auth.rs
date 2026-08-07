//! Port of `src/utils/auth.ts` — OAuth token read/write and logout.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::constants::oauth::should_use_claude_ai_auth;
use crate::utils::config::{get_global_config, save_global_config};
use crate::utils::env_utils::is_bare_mode;
use crate::utils::secure_storage::{self};

pub use crate::utils::secure_storage::OAuthTokens;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OauthAccountInfo {
    #[serde(default)]
    pub email_address: Option<String>,
    #[serde(default)]
    pub organization_uuid: Option<String>,
    #[serde(default)]
    pub organization_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthTokenSource {
    None,
    AnthropicAuthToken,
    ClaudeCodeOauthToken,
    ClaudeAi,
    ApiKeyHelper,
}

impl AuthTokenSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AnthropicAuthToken => "ANTHROPIC_AUTH_TOKEN",
            Self::ClaudeCodeOauthToken => "CLAUDE_CODE_OAUTH_TOKEN",
            Self::ClaudeAi => "claude.ai",
            Self::ApiKeyHelper => "apiKeyHelper",
        }
    }
}

/// Read Claude.ai OAuth tokens (env override or secure storage).
pub fn get_claude_ai_oauth_tokens() -> Option<OAuthTokens> {
    if is_bare_mode() {
        return None;
    }

    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
        if !token.is_empty() {
            return Some(OAuthTokens {
                access_token: token,
                refresh_token: None,
                expires_at: None,
                scopes: vec!["user:inference".into()],
                subscription_type: None,
                rate_limit_tiers: None,
            });
        }
    }

    let storage = secure_storage::read()?;
    let oauth = storage.claude_ai_oauth?;
    if oauth.access_token.is_empty() {
        return None;
    }
    Some(oauth)
}

pub fn clear_oauth_token_cache() {
    // No memoize cache yet; placeholder for parity with TS.
}

/// Persist OAuth tokens to secure storage (Claude.ai inference scopes).
pub fn save_oauth_tokens_if_needed(tokens: &OAuthTokens) -> Result<(), String> {
    if !should_use_claude_ai_auth(Some(&tokens.scopes)) {
        return Err("Token scopes do not include user:inference".into());
    }
    let mut data = secure_storage::read().unwrap_or_default();
    data.claude_ai_oauth = Some(tokens.clone());
    secure_storage::update(&data)?;
    clear_oauth_token_cache();
    Ok(())
}

pub fn get_auth_token_source() -> (AuthTokenSource, bool) {
    if is_bare_mode() {
        return (AuthTokenSource::None, false);
    }

    if std::env::var_os("ANTHROPIC_AUTH_TOKEN").is_some() {
        return (AuthTokenSource::AnthropicAuthToken, true);
    }
    if std::env::var_os("CLAUDE_CODE_OAUTH_TOKEN").is_some() {
        return (AuthTokenSource::ClaudeCodeOauthToken, true);
    }

    let oauth_tokens = get_claude_ai_oauth_tokens();
    if should_use_claude_ai_auth(oauth_tokens.as_ref().map(|t| t.scopes.as_slice()))
        && oauth_tokens
            .as_ref()
            .map(|t| !t.access_token.is_empty())
            .unwrap_or(false)
    {
        return (AuthTokenSource::ClaudeAi, true);
    }

    (AuthTokenSource::None, false)
}

pub fn get_oauth_account_info() -> Option<OauthAccountInfo> {
    let cfg = get_global_config();
    if let Some(account) = cfg.oauth_account.clone() {
        return Some(account);
    }
    cfg.extra
        .get("oauthAccount")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub fn store_oauth_account_info(account: OauthAccountInfo) {
    save_global_config(|mut current| {
        current.oauth_account = Some(account);
        current.extra.remove("oauthAccount");
        current
    });
}

/// Clear secure storage + oauthAccount from global config.
pub fn perform_logout() -> Result<(), String> {
    if !secure_storage::delete() {
        return Err("Failed to delete credentials file".into());
    }
    clear_oauth_token_cache();
    save_global_config(|mut current| {
        current.oauth_account = None;
        current.extra.remove("oauthAccount");
        current.extra.remove("primaryApiKey");
        current
    });
    Ok(())
}

pub fn has_anthropic_api_key_env() -> bool {
    std::env::var_os("ANTHROPIC_API_KEY").is_some_and(|v| !v.is_empty())
}

/// Redacted status payload (never includes access tokens).
pub fn auth_status_json() -> serde_json::Value {
    let (source, has_token) = get_auth_token_source();
    let has_api_key = has_anthropic_api_key_env();
    let logged_in = has_token || has_api_key;

    let mut auth_method = "none".to_string();
    if source == AuthTokenSource::ClaudeAi {
        auth_method = "claude.ai".into();
    } else if has_token && source != AuthTokenSource::None {
        auth_method = "oauth_token".into();
    } else if has_api_key {
        auth_method = "api_key".into();
    }

    let mut output = json!({
        "loggedIn": logged_in,
        "authMethod": auth_method,
        "apiProvider": "firstParty",
    });

    if has_api_key {
        output["apiKeySource"] = json!("ANTHROPIC_API_KEY");
    }

    if auth_method == "claude.ai" {
        let account = get_oauth_account_info();
        output["email"] = json!(account.as_ref().and_then(|a| a.email_address.clone()));
        output["orgId"] = json!(account.as_ref().and_then(|a| a.organization_uuid.clone()));
        output["orgName"] = json!(account.as_ref().and_then(|a| a.organization_name.clone()));
        let tokens = get_claude_ai_oauth_tokens();
        output["subscriptionType"] =
            json!(tokens.as_ref().and_then(|t| t.subscription_type.clone()));
    }

    output
}
