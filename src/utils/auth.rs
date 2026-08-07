//! Port of `src/utils/auth.ts` (stub).

#[derive(Debug, Clone, Default)]
pub struct OAuthTokens {
    pub access_token: Option<String>,
}

pub fn get_claude_ai_oauth_tokens() -> Option<OAuthTokens> {
    None
}
