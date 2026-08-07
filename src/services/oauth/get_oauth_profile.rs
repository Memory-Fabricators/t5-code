//! Port of `src/services/oauth/getOauthProfile.ts`.

use serde::Deserialize;

use crate::constants::oauth::BASE_API_URL;

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProfileAccount {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProfileOrganization {
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub organization_type: Option<String>,
    #[serde(default)]
    pub rate_limit_tier: Option<String>,
    #[serde(default)]
    pub has_extra_usage_enabled: Option<bool>,
    #[serde(default)]
    pub billing_type: Option<String>,
    #[serde(default)]
    pub subscription_created_at: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProfileResponse {
    #[serde(default)]
    pub account: Option<OAuthProfileAccount>,
    #[serde(default)]
    pub organization: Option<OAuthProfileOrganization>,
}

pub fn profile_url() -> String {
    format!("{BASE_API_URL}/api/oauth/profile")
}

/// GET /api/oauth/profile with Bearer access token. Errors → None.
pub async fn get_oauth_profile_from_oauth_token(
    access_token: &str,
) -> Option<OAuthProfileResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let response = client
        .get(profile_url())
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    response.json().await.ok()
}
