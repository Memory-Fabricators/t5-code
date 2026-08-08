//! Port of `src/services/oauth/index.ts` — authorization-code + PKCE flow.

use tokio::sync::mpsc;

use crate::PortResult;
use crate::services::oauth::auth_code_listener::AuthCodeListener;
use crate::services::oauth::client::{
    BuildAuthUrlOpts, build_auth_url, exchange_code_for_tokens, fetch_profile_info, parse_scopes,
};
use crate::services::oauth::crypto::{
    generate_code_challenge, generate_code_verifier, generate_state,
};
use crate::utils::auth::OAuthTokens;
use crate::utils::browser::open_browser;

pub struct OAuthFlowOptions {
    pub login_with_claude_ai: bool,
    pub inference_only: bool,
    pub expires_in: Option<u64>,
    pub org_uuid: Option<String>,
    pub login_hint: Option<String>,
    pub login_method: Option<String>,
    pub skip_browser_open: bool,
}

impl Default for OAuthFlowOptions {
    fn default() -> Self {
        Self {
            login_with_claude_ai: true,
            inference_only: false,
            expires_in: None,
            org_uuid: None,
            login_hint: None,
            login_method: None,
            skip_browser_open: false,
        }
    }
}

/// OAuth service: PKCE authorize → localhost (or manual paste) → token exchange.
pub struct OAuthService {
    code_verifier: String,
}

impl OAuthService {
    pub fn new() -> Self {
        Self {
            code_verifier: generate_code_verifier(),
        }
    }

    /// Start the browser OAuth flow.
    ///
    /// `auth_url_handler(manual_url, automatic_url)` shows the manual URL.
    /// If `manual_rx` is provided, lines received are treated as pasted auth codes
    /// (`code#state` or bare code).
    pub async fn start_oauth_flow<F, Fut>(
        &self,
        auth_url_handler: F,
        options: OAuthFlowOptions,
        mut manual_rx: Option<mpsc::UnboundedReceiver<String>>,
    ) -> PortResult<OAuthTokens>
    where
        F: FnOnce(String, Option<String>) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let mut listener = AuthCodeListener::new();
        let port = listener.start(None).await?;

        let code_challenge = generate_code_challenge(&self.code_verifier);
        let state = generate_state();

        let base_opts = BuildAuthUrlOpts {
            code_challenge,
            state: state.clone(),
            port,
            is_manual: false,
            login_with_claude_ai: options.login_with_claude_ai,
            inference_only: options.inference_only,
            org_uuid: options.org_uuid.clone(),
            login_hint: options.login_hint.clone(),
            login_method: options.login_method.clone(),
        };
        let automatic_flow_url = build_auth_url(&base_opts);
        let manual_flow_url = build_auth_url(&BuildAuthUrlOpts {
            is_manual: true,
            ..base_opts
        });

        let skip_browser = options.skip_browser_open;
        let auto_fut = listener.wait_for_authorization(&state, || async {
            if skip_browser {
                auth_url_handler(manual_flow_url, Some(automatic_flow_url)).await;
            } else {
                auth_url_handler(manual_flow_url, None).await;
                let _ = open_browser(&automatic_flow_url);
            }
        });
        tokio::pin!(auto_fut);

        let (authorization_code, mut is_automatic) = if let Some(ref mut rx) = manual_rx {
            loop {
                tokio::select! {
                    result = &mut auto_fut => {
                        break (result?, true);
                    }
                    Some(line) = rx.recv() => {
                        if let Some((code, _st)) = parse_manual_code_paste(&line) {
                            listener.close().await;
                            break (code, false);
                        }
                        if !line.trim().is_empty() {
                            listener.close().await;
                            break (line.trim().to_string(), false);
                        }
                    }
                }
            }
        } else {
            (auto_fut.await?, true)
        };

        if is_automatic {
            is_automatic = listener.has_pending_response().await;
        }

        let exchange_result = exchange_code_for_tokens(
            &authorization_code,
            &state,
            &self.code_verifier,
            port,
            !is_automatic,
            options.expires_in,
        )
        .await;

        let token_response = match exchange_result {
            Ok(r) => r,
            Err(e) => {
                if is_automatic {
                    listener.handle_error_redirect().await;
                }
                listener.close().await;
                return Err(e);
            }
        };

        let profile_info = fetch_profile_info(&token_response.access_token).await;
        let scopes = parse_scopes(token_response.scope.as_deref());

        if is_automatic {
            listener.handle_success_redirect(&scopes).await;
        }
        listener.close().await;

        let expires_at = token_response.expires_in.map(|secs| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            now + (secs as i64) * 1000
        });

        Ok(OAuthTokens {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_at,
            scopes,
            subscription_type: profile_info
                .as_ref()
                .and_then(|p| p.subscription_type.clone()),
            rate_limit_tiers: profile_info
                .as_ref()
                .and_then(|p| p.rate_limit_tiers.clone()),
        })
    }
}

impl Default for OAuthService {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse pasted `authorizationCode#state` (manual redirect flow).
pub fn parse_manual_code_paste(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    let (code, state) = trimmed.split_once('#')?;
    if code.is_empty() || state.is_empty() {
        return None;
    }
    Some((code.to_string(), state.to_string()))
}
