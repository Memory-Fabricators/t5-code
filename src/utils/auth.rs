//! Port of `src/utils/auth.ts` — OAuth token read/write, refresh gate, logout.

use std::fs::{self, File, OpenOptions};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex as AsyncMutex;

use crate::constants::oauth::should_use_claude_ai_auth;
use crate::services::oauth::client::{is_oauth_token_expired, refresh_oauth_token};
use crate::utils::config::{get_global_config, save_global_config};
use crate::utils::env_utils::{get_claude_config_home_dir, is_bare_mode};
use crate::utils::secure_storage::{self, credentials_path};

pub use crate::utils::secure_storage::OAuthTokens;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OauthAccountInfo {
    #[serde(default)]
    pub account_uuid: Option<String>,
    #[serde(default)]
    pub email_address: Option<String>,
    #[serde(default)]
    pub organization_uuid: Option<String>,
    #[serde(default)]
    pub organization_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub billing_type: Option<String>,
    #[serde(default)]
    pub has_extra_usage_enabled: Option<bool>,
    #[serde(default)]
    pub account_created_at: Option<String>,
    #[serde(default)]
    pub subscription_created_at: Option<String>,
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

static LAST_CREDENTIALS_MTIME_MS: Mutex<u64> = Mutex::new(0);
static PENDING_REFRESH: AsyncMutex<Option<()>> = AsyncMutex::const_new(None);

/// Read Claude.ai OAuth tokens (env override or secure storage).
pub fn get_claude_ai_oauth_tokens() -> Option<OAuthTokens> {
    if is_bare_mode() {
        return None;
    }

    if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        && !token.is_empty()
    {
        return Some(OAuthTokens {
            access_token: token,
            refresh_token: None,
            expires_at: None,
            scopes: vec!["user:inference".into()],
            subscription_type: None,
            rate_limit_tiers: None,
        });
    }

    let storage = secure_storage::read()?;
    let oauth = storage.claude_ai_oauth?;
    if oauth.access_token.is_empty() {
        return None;
    }
    Some(oauth)
}

pub fn clear_oauth_token_cache() {
    // No memoize cache yet; mtime tracking still used for cross-process refresh.
}

fn invalidate_oauth_cache_if_disk_changed() {
    let path = credentials_path();
    let Ok(meta) = fs::metadata(&path) else {
        return;
    };
    #[cfg(unix)]
    let mtime_ms = {
        use std::os::unix::fs::MetadataExt;
        (meta.mtime() as u64).saturating_mul(1000)
    };
    #[cfg(not(unix))]
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut last = LAST_CREDENTIALS_MTIME_MS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if *last != mtime_ms {
        *last = mtime_ms;
        clear_oauth_token_cache();
    }
}

/// Persist OAuth tokens; preserve existing subscription/rateLimit on null.
pub fn save_oauth_tokens_if_needed(tokens: &OAuthTokens) -> Result<(), String> {
    if !should_use_claude_ai_auth(Some(&tokens.scopes)) {
        return Ok(());
    }
    if tokens
        .refresh_token
        .as_ref()
        .map(|s| s.is_empty())
        .unwrap_or(true)
        || tokens.expires_at.is_none()
    {
        // Match TS: inference-only tokens without refresh+expiry are not persisted.
        if tokens.refresh_token.is_none() || tokens.expires_at.is_none() {
            return Ok(());
        }
    }

    let mut data = secure_storage::read().unwrap_or_default();
    let existing = data.claude_ai_oauth.clone();
    let mut to_store = tokens.clone();
    if to_store.subscription_type.is_none() {
        to_store.subscription_type = existing.as_ref().and_then(|e| e.subscription_type.clone());
    }
    if to_store.rate_limit_tiers.is_none() {
        to_store.rate_limit_tiers = existing.as_ref().and_then(|e| e.rate_limit_tiers.clone());
    }
    data.claude_ai_oauth = Some(to_store);
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

pub fn is_claude_ai_subscriber() -> bool {
    let tokens = get_claude_ai_oauth_tokens();
    should_use_claude_ai_auth(tokens.as_ref().map(|t| t.scopes.as_slice()))
}

pub fn has_anthropic_api_key_env() -> bool {
    std::env::var_os("ANTHROPIC_API_KEY").is_some_and(|v| !v.is_empty())
}

struct RefreshDirLock {
    _file: File,
    path: std::path::PathBuf,
}

impl RefreshDirLock {
    fn try_acquire() -> Result<Self, bool> {
        // true = locked by other (retry), Err(false) = hard failure
        let dir = get_claude_config_home_dir();
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(".oauth-refresh.lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => Ok(Self { _file: file, path }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(true),
            Err(_) => Err(false),
        }
    }
}

impl Drop for RefreshDirLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

const MAX_REFRESH_RETRIES: u32 = 5;

/// Refresh OAuth tokens if expired (dir lock + cross-process race handling).
/// Returns true if tokens were refreshed.
pub async fn check_and_refresh_oauth_token_if_needed(retry_count: u32, force: bool) -> bool {
    // Deduplicate concurrent non-retry, non-force calls within this process.
    if retry_count == 0 && !force {
        let mut pending = PENDING_REFRESH.lock().await;
        if pending.is_some() {
            // Another call is in flight; wait briefly and re-check disk.
            drop(pending);
            tokio::time::sleep(Duration::from_millis(50)).await;
            invalidate_oauth_cache_if_disk_changed();
            return false;
        }
        *pending = Some(());
        drop(pending);
        let result = check_and_refresh_oauth_token_if_needed_impl(retry_count, force).await;
        let mut pending = PENDING_REFRESH.lock().await;
        *pending = None;
        return result;
    }
    check_and_refresh_oauth_token_if_needed_impl(retry_count, force).await
}

async fn check_and_refresh_oauth_token_if_needed_impl(retry_count: u32, force: bool) -> bool {
    let mut retry_count = retry_count;
    loop {
        invalidate_oauth_cache_if_disk_changed();

        let tokens = match get_claude_ai_oauth_tokens() {
            Some(t) => t,
            None => return false,
        };

        if !force && (tokens.refresh_token.is_none() || !is_oauth_token_expired(tokens.expires_at))
        {
            return false;
        }

        let Some(refresh_token) = tokens.refresh_token.clone() else {
            return false;
        };

        if !should_use_claude_ai_auth(Some(&tokens.scopes)) {
            return false;
        }

        clear_oauth_token_cache();
        invalidate_oauth_cache_if_disk_changed();
        let tokens = match get_claude_ai_oauth_tokens() {
            Some(t) => t,
            None => return false,
        };
        if tokens.refresh_token.is_none() || !is_oauth_token_expired(tokens.expires_at) {
            return false;
        }

        let lock = match RefreshDirLock::try_acquire() {
            Ok(l) => l,
            Err(true) => {
                if retry_count >= MAX_REFRESH_RETRIES {
                    return false;
                }
                let jitter_ms = 1000 + (rand_jitter_ms() % 1000);
                tokio::time::sleep(Duration::from_millis(jitter_ms)).await;
                retry_count += 1;
                continue;
            }
            Err(false) => return false,
        };

        clear_oauth_token_cache();
        let locked_tokens = match get_claude_ai_oauth_tokens() {
            Some(t) => t,
            None => {
                drop(lock);
                return false;
            }
        };
        if !is_oauth_token_expired(locked_tokens.expires_at) {
            drop(lock);
            return false;
        }
        let Some(refresh_token) = locked_tokens.refresh_token.clone().or(Some(refresh_token))
        else {
            drop(lock);
            return false;
        };

        let scopes = if should_use_claude_ai_auth(Some(&locked_tokens.scopes)) {
            None
        } else {
            Some(locked_tokens.scopes.clone())
        };

        return match refresh_oauth_token(&refresh_token, scopes.as_deref()).await {
            Ok(refreshed) => {
                let _ = save_oauth_tokens_if_needed(&refreshed);
                clear_oauth_token_cache();
                drop(lock);
                true
            }
            Err(_) => {
                clear_oauth_token_cache();
                let recovered = get_claude_ai_oauth_tokens()
                    .map(|t| !is_oauth_token_expired(t.expires_at))
                    .unwrap_or(false);
                drop(lock);
                recovered
            }
        };
    }
}

fn rand_jitter_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
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
