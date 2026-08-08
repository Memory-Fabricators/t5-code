//! Port of `src/services/oauth/client.ts` — refresh + profile populate.

use serde::Deserialize;

use crate::PortResult;
use crate::constants::oauth::{
    ALL_OAUTH_SCOPES, API_KEY_URL, CLAUDE_AI_AUTHORIZE_URL, CLAUDE_AI_INFERENCE_SCOPE,
    CLAUDE_AI_OAUTH_SCOPES, CLAUDE_AI_PROFILE_SCOPE, CLIENT_ID, CONSOLE_AUTHORIZE_URL,
    MANUAL_REDIRECT_URL, ROLES_URL, TOKEN_URL, should_use_claude_ai_auth,
};
use crate::services::oauth::get_oauth_profile::get_oauth_profile_from_oauth_token;
use crate::utils::auth::{
    OAuthTokens, OauthAccountInfo, get_claude_ai_oauth_tokens, get_oauth_account_info,
    store_oauth_account_info,
};

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub subscription_type: Option<String>,
    pub display_name: Option<String>,
    pub rate_limit_tiers: Option<serde_json::Value>,
    pub has_extra_usage_enabled: Option<bool>,
    pub billing_type: Option<String>,
    pub account_created_at: Option<String>,
    pub subscription_created_at: Option<String>,
    pub account_uuid: Option<String>,
    pub email: Option<String>,
    pub organization_uuid: Option<String>,
    pub organization_name: Option<String>,
}

pub fn is_oauth_token_expired(expires_at: Option<i64>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    let buffer_ms = 5 * 60 * 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    now + buffer_ms >= expires_at
}

pub fn parse_scopes(scope: Option<&str>) -> Vec<String> {
    scope
        .unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone)]
pub struct BuildAuthUrlOpts {
    pub code_challenge: String,
    pub state: String,
    pub port: u16,
    pub is_manual: bool,
    pub login_with_claude_ai: bool,
    pub inference_only: bool,
    pub org_uuid: Option<String>,
    pub login_hint: Option<String>,
    pub login_method: Option<String>,
}

pub fn build_auth_url(opts: &BuildAuthUrlOpts) -> String {
    let auth_url_base = if opts.login_with_claude_ai {
        CLAUDE_AI_AUTHORIZE_URL
    } else {
        CONSOLE_AUTHORIZE_URL
    };

    let redirect_uri = if opts.is_manual {
        MANUAL_REDIRECT_URL.to_string()
    } else {
        format!("http://localhost:{}/callback", opts.port)
    };

    let scopes: Vec<&str> = if opts.inference_only {
        vec![CLAUDE_AI_INFERENCE_SCOPE]
    } else {
        ALL_OAUTH_SCOPES.to_vec()
    };

    let mut params: Vec<(String, String)> = vec![
        ("code".into(), "true".into()),
        ("client_id".into(), CLIENT_ID.to_string()),
        ("response_type".into(), "code".into()),
        ("redirect_uri".into(), redirect_uri),
        ("scope".into(), scopes.join(" ")),
        ("code_challenge".into(), opts.code_challenge.clone()),
        ("code_challenge_method".into(), "S256".into()),
        ("state".into(), opts.state.clone()),
    ];
    if let Some(org) = &opts.org_uuid {
        params.push(("orgUUID".into(), org.clone()));
    }
    if let Some(hint) = &opts.login_hint {
        params.push(("login_hint".into(), hint.clone()));
    }
    if let Some(method) = &opts.login_method {
        params.push(("login_method".into(), method.clone()));
    }

    let query: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", form_urlencoded(k), form_urlencoded(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{auth_url_base}?{query}")
}

fn form_urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
pub struct OAuthTokenExchangeResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub account: Option<TokenAccount>,
    #[serde(default)]
    pub organization: Option<TokenOrganization>,
}

#[derive(Debug, Deserialize)]
pub struct TokenAccount {
    pub uuid: String,
    #[serde(default)]
    pub email_address: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TokenOrganization {
    pub uuid: String,
}

pub async fn exchange_code_for_tokens(
    authorization_code: &str,
    state: &str,
    code_verifier: &str,
    port: u16,
    use_manual_redirect: bool,
    expires_in: Option<u64>,
) -> PortResult<OAuthTokenExchangeResponse> {
    let redirect_uri = if use_manual_redirect {
        MANUAL_REDIRECT_URL.to_string()
    } else {
        format!("http://localhost:{port}/callback")
    };

    let mut body = serde_json::json!({
        "grant_type": "authorization_code",
        "code": authorization_code,
        "redirect_uri": redirect_uri,
        "client_id": CLIENT_ID,
        "code_verifier": code_verifier,
        "state": state,
    });
    if let Some(exp) = expires_in {
        body["expires_in"] = serde_json::json!(exp);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        let msg = if status.as_u16() == 401 {
            "Authentication failed: Invalid authorization code".into()
        } else {
            format!("Token exchange failed ({status}): {text}")
        };
        return Err(rootcause::report!(msg).into());
    }

    Ok(response.json().await?)
}

pub async fn fetch_and_store_user_roles(access_token: &str) -> PortResult<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let response = client
        .get(ROLES_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(rootcause::report!(format!("Failed to fetch user roles: {status}")).into());
    }
    let data: serde_json::Value = response.json().await?;
    let org_role = data
        .get("organization_role")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let workspace_role = data
        .get("workspace_role")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let organization_name = data
        .get("organization_name")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let Some(mut account) = get_oauth_account_info() else {
        return Err(rootcause::report!("OAuth account information not found in config").into());
    };
    if let Some(v) = org_role {
        account
            .extra
            .insert("organizationRole".into(), serde_json::json!(v));
    }
    if let Some(v) = workspace_role {
        account
            .extra
            .insert("workspaceRole".into(), serde_json::json!(v));
    }
    if let Some(v) = organization_name {
        account.organization_name = Some(v);
    }
    store_oauth_account_info(account);
    Ok(())
}

pub async fn create_and_store_api_key(access_token: &str) -> PortResult<Option<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let response = client
        .post(API_KEY_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(rootcause::report!(format!("API key creation failed: {status} {text}")).into());
    }
    let data: serde_json::Value = response.json().await?;
    let api_key = data
        .get("raw_key")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(ref key) = api_key {
        crate::utils::auth::save_api_key(key).map_err(|e| rootcause::report!(e))?;
    }
    Ok(api_key)
}

fn map_subscription_type(org_type: Option<&str>) -> Option<String> {
    match org_type {
        Some("claude_max") => Some("max".into()),
        Some("claude_pro") => Some("pro".into()),
        Some("claude_enterprise") => Some("enterprise".into()),
        Some("claude_team") => Some("team".into()),
        _ => None,
    }
}

pub async fn fetch_profile_info(access_token: &str) -> Option<ProfileInfo> {
    let profile = get_oauth_profile_from_oauth_token(access_token).await?;
    let org = profile.organization;
    let account = profile.account;

    let subscription_type =
        map_subscription_type(org.as_ref().and_then(|o| o.organization_type.as_deref()));
    let rate_limit_tiers = org
        .as_ref()
        .and_then(|o| o.rate_limit_tier.clone())
        .map(serde_json::Value::String);

    Some(ProfileInfo {
        subscription_type,
        display_name: account.as_ref().and_then(|a| a.display_name.clone()),
        rate_limit_tiers,
        has_extra_usage_enabled: org.as_ref().and_then(|o| o.has_extra_usage_enabled),
        billing_type: org.as_ref().and_then(|o| o.billing_type.clone()),
        account_created_at: account.as_ref().and_then(|a| a.created_at.clone()),
        subscription_created_at: org.as_ref().and_then(|o| o.subscription_created_at.clone()),
        account_uuid: account.as_ref().and_then(|a| a.uuid.clone()),
        email: account.as_ref().and_then(|a| a.email.clone()),
        organization_uuid: org.as_ref().and_then(|o| o.uuid.clone()),
        organization_name: org.as_ref().and_then(|o| o.name.clone()),
    })
}

fn profile_already_complete() -> bool {
    let Some(account) = get_oauth_account_info() else {
        return false;
    };
    let tokens = get_claude_ai_oauth_tokens();
    account.billing_type.is_some()
        && account.account_created_at.is_some()
        && account.subscription_created_at.is_some()
        && tokens
            .as_ref()
            .and_then(|t| t.subscription_type.as_ref())
            .is_some()
        && tokens
            .as_ref()
            .and_then(|t| t.rate_limit_tiers.as_ref())
            .is_some()
}

/// Exchange a refresh token for new access/refresh tokens; optionally enrich from profile.
pub async fn refresh_oauth_token(
    refresh_token: &str,
    requested_scopes: Option<&[String]>,
) -> PortResult<OAuthTokens> {
    let scopes: Vec<String> = match requested_scopes {
        Some(s) if !s.is_empty() => s.to_vec(),
        _ => CLAUDE_AI_OAUTH_SCOPES
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": CLIENT_ID,
        "scope": scopes.join(" "),
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(rootcause::report!(format!("Token refresh failed: {status} {text}")).into());
    }

    let data: TokenExchangeResponse = response.json().await?;
    let expires_at = data.expires_in.map(|secs| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        now + (secs as i64) * 1000
    });

    let resolved_scopes = {
        let parsed = parse_scopes(data.scope.as_deref());
        if parsed.is_empty() { scopes } else { parsed }
    };

    let mut tokens = OAuthTokens {
        access_token: data.access_token,
        refresh_token: data
            .refresh_token
            .or_else(|| Some(refresh_token.to_string())),
        expires_at,
        scopes: resolved_scopes,
        subscription_type: None,
        rate_limit_tiers: None,
    };

    // Enrich from profile unless already complete
    if !profile_already_complete() {
        if let Some(info) = fetch_profile_info(&tokens.access_token).await {
            tokens.subscription_type = info.subscription_type.clone();
            tokens.rate_limit_tiers = info.rate_limit_tiers.clone();
            apply_profile_to_account(&info);
        }
    } else {
        // Preserve existing subscription metadata from storage
        if let Some(existing) = get_claude_ai_oauth_tokens() {
            tokens.subscription_type = existing.subscription_type;
            tokens.rate_limit_tiers = existing.rate_limit_tiers;
        }
    }

    Ok(tokens)
}

fn apply_profile_to_account(info: &ProfileInfo) {
    let mut account = get_oauth_account_info().unwrap_or(OauthAccountInfo {
        account_uuid: None,
        email_address: None,
        organization_uuid: None,
        organization_name: None,
        display_name: None,
        billing_type: None,
        has_extra_usage_enabled: None,
        account_created_at: None,
        subscription_created_at: None,
        extra: serde_json::Map::new(),
    });

    if let Some(v) = &info.account_uuid {
        account.account_uuid = Some(v.clone());
    }
    if let Some(v) = &info.email {
        account.email_address = Some(v.clone());
    }
    if let Some(v) = &info.organization_uuid {
        account.organization_uuid = Some(v.clone());
    }
    if let Some(v) = &info.organization_name {
        account.organization_name = Some(v.clone());
    }
    if let Some(v) = &info.display_name {
        account.display_name = Some(v.clone());
    }
    if let Some(v) = &info.billing_type {
        account.billing_type = Some(v.clone());
    }
    if let Some(v) = info.has_extra_usage_enabled {
        account.has_extra_usage_enabled = Some(v);
    }
    if let Some(v) = &info.account_created_at {
        account.account_created_at = Some(v.clone());
    }
    if let Some(v) = &info.subscription_created_at {
        account.subscription_created_at = Some(v.clone());
    }

    store_oauth_account_info(account);
}

pub fn has_profile_scope(scopes: Option<&[String]>) -> bool {
    scopes
        .map(|s| s.iter().any(|scope| scope == CLAUDE_AI_PROFILE_SCOPE))
        .unwrap_or(false)
}

/// Ensure oauthAccount is populated (env fallback → refresh → profile GET).
pub async fn populate_oauth_account_info_if_needed() -> bool {
    // Env fallback
    if get_oauth_account_info().is_none() {
        let uuid = std::env::var("CLAUDE_CODE_ACCOUNT_UUID").ok();
        let email = std::env::var("CLAUDE_CODE_USER_EMAIL").ok();
        let org = std::env::var("CLAUDE_CODE_ORGANIZATION_UUID").ok();
        if uuid.is_some() && email.is_some() && org.is_some() {
            store_oauth_account_info(OauthAccountInfo {
                account_uuid: uuid,
                email_address: email,
                organization_uuid: org,
                organization_name: None,
                display_name: None,
                billing_type: None,
                has_extra_usage_enabled: None,
                account_created_at: None,
                subscription_created_at: None,
                extra: serde_json::Map::new(),
            });
        }
    }

    let _ = crate::utils::auth::check_and_refresh_oauth_token_if_needed(0, false).await;

    let account = get_oauth_account_info();
    if account
        .as_ref()
        .map(|a| {
            a.billing_type.is_some()
                && a.account_created_at.is_some()
                && a.subscription_created_at.is_some()
        })
        .unwrap_or(false)
    {
        return false;
    }

    let tokens = match get_claude_ai_oauth_tokens() {
        Some(t)
            if should_use_claude_ai_auth(Some(&t.scopes)) && has_profile_scope(Some(&t.scopes)) =>
        {
            t
        }
        _ => return false,
    };

    let Some(info) = fetch_profile_info(&tokens.access_token).await else {
        return false;
    };
    apply_profile_to_account(&info);

    // Also update token metadata if missing
    if tokens.subscription_type.is_none() || tokens.rate_limit_tiers.is_none() {
        let mut updated = tokens;
        if updated.subscription_type.is_none() {
            updated.subscription_type = info.subscription_type;
        }
        if updated.rate_limit_tiers.is_none() {
            updated.rate_limit_tiers = info.rate_limit_tiers;
        }
        let _ = crate::utils::auth::save_oauth_tokens_if_needed(&updated);
    }

    true
}
