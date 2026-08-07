//! Port of `src/services/oauth/client.ts` — token refresh (browser PKCE deferred).

use serde::Deserialize;

use crate::PortResult;
use crate::constants::oauth::{CLAUDE_AI_OAUTH_SCOPES, CLIENT_ID, TOKEN_URL};
use crate::utils::auth::OAuthTokens;

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

fn parse_scopes(scope: Option<&str>) -> Vec<String> {
    scope
        .unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Exchange a refresh token for new access/refresh tokens.
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

    Ok(OAuthTokens {
        access_token: data.access_token,
        refresh_token: data
            .refresh_token
            .or_else(|| Some(refresh_token.to_string())),
        expires_at,
        scopes: resolved_scopes,
        subscription_type: None,
        rate_limit_tiers: None,
    })
}
