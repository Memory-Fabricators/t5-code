//! Port of `src/cli/handlers/auth.ts`.

use std::io::{self, BufRead, Write};

use rootcause::Report;
use tokio::sync::mpsc;

use crate::PortResult;
use crate::constants::oauth::should_use_claude_ai_auth;
use crate::services::oauth::client::{
    create_and_store_api_key, fetch_and_store_user_roles, fetch_profile_info,
    populate_oauth_account_info_if_needed, refresh_oauth_token,
};
use crate::services::oauth::flow::{OAuthFlowOptions, OAuthService};
use crate::utils::auth::{
    AuthTokenSource, OAuthTokens, OauthAccountInfo, auth_status_json,
    check_and_refresh_oauth_token_if_needed, get_auth_token_source, perform_logout,
    save_oauth_tokens_if_needed, store_oauth_account_info,
};
use crate::utils::config::save_global_config;

fn report_msg(msg: impl Into<String>) -> Report {
    rootcause::report!(msg.into()).into()
}

/// Shared post-token-acquisition logic (port of `installOAuthTokens`).
pub async fn install_oauth_tokens(tokens: &OAuthTokens) -> PortResult<()> {
    let _ = perform_logout();

    if let Some(info) = fetch_profile_info(&tokens.access_token).await {
        store_oauth_account_info(OauthAccountInfo {
            account_uuid: info.account_uuid,
            email_address: info.email,
            organization_uuid: info.organization_uuid,
            organization_name: info.organization_name,
            display_name: info.display_name,
            billing_type: info.billing_type,
            has_extra_usage_enabled: info.has_extra_usage_enabled,
            account_created_at: info.account_created_at,
            subscription_created_at: info.subscription_created_at,
            extra: serde_json::Map::new(),
        });
    }

    save_oauth_tokens_if_needed(tokens).map_err(report_msg)?;

    let _ = fetch_and_store_user_roles(&tokens.access_token).await;

    if should_use_claude_ai_auth(Some(&tokens.scopes)) {
        // First-token-date deferred.
    } else {
        let api_key = create_and_store_api_key(&tokens.access_token).await?;
        if api_key.is_none() {
            return Err(report_msg(
                "Unable to create API key. The server accepted the request but did not return a key.",
            )
            .into());
        }
    }

    Ok(())
}

fn mark_onboarding_complete() {
    save_global_config(|mut current| {
        if current
            .extra
            .get("hasCompletedOnboarding")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return current;
        }
        current
            .extra
            .insert("hasCompletedOnboarding".into(), serde_json::json!(true));
        current
    });
}

pub async fn auth_status(json: bool) -> PortResult<()> {
    let _ = check_and_refresh_oauth_token_if_needed(0, false).await;
    let _ = populate_oauth_account_info_if_needed().await;

    let output = auth_status_json();
    let logged_in = output
        .get("loggedIn")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let (source, has_token) = get_auth_token_source();
        if has_token {
            match source {
                AuthTokenSource::ClaudeAi => {
                    if let Some(email) = output.get("email").and_then(|v| v.as_str()) {
                        println!("Logged in as: {email}");
                    } else {
                        println!("Logged in via claude.ai");
                    }
                    if let Some(sub) = output.get("subscriptionType").and_then(|v| v.as_str()) {
                        println!("Subscription: {sub}");
                    }
                }
                AuthTokenSource::ClaudeCodeOauthToken => {
                    println!("Auth: CLAUDE_CODE_OAUTH_TOKEN");
                }
                AuthTokenSource::AnthropicAuthToken => {
                    println!("Auth: ANTHROPIC_AUTH_TOKEN");
                }
                _ => println!("Logged in"),
            }
        }
        if output.get("apiKeySource").is_some() {
            println!("API key: ANTHROPIC_API_KEY");
        }
        if !logged_in {
            println!("Not logged in. Run t5-code auth login to authenticate.");
        }
    }

    if logged_in {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

pub async fn auth_logout() -> PortResult<()> {
    perform_logout().map_err(report_msg)?;
    println!("Successfully logged out from your Anthropic account.");
    Ok(())
}

pub async fn auth_login() -> PortResult<()> {
    if let Ok(env_refresh) = std::env::var("CLAUDE_CODE_OAUTH_REFRESH_TOKEN")
        && !env_refresh.is_empty()
    {
        let env_scopes = std::env::var("CLAUDE_CODE_OAUTH_SCOPES").map_err(|_| {
            report_msg(
                "CLAUDE_CODE_OAUTH_SCOPES is required when using CLAUDE_CODE_OAUTH_REFRESH_TOKEN.\n\
                     Set it to the space-separated scopes the refresh token was issued with\n\
                     (e.g. \"user:inference\" or \"user:profile user:inference user:sessions:claude_code user:mcp_servers\").",
            )
        })?;
        let scopes: Vec<String> = env_scopes
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();

        let tokens = refresh_oauth_token(&env_refresh, Some(&scopes)).await?;
        install_oauth_tokens(&tokens).await?;
        mark_onboarding_complete();
        println!("Login successful.");
        return Ok(());
    }

    let (paste_tx, paste_rx) = mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            if paste_tx.send(line).is_err() {
                break;
            }
        }
    });

    let oauth = OAuthService::new();
    let options = OAuthFlowOptions {
        login_with_claude_ai: true,
        ..Default::default()
    };

    let tokens = oauth
        .start_oauth_flow(
            |manual_url, _auto| async move {
                println!("Opening browser to sign in…");
                println!("If the browser didn't open, visit:\n  {manual_url}");
                println!();
                print!("Or paste the authorization code here: ");
                let _ = io::stdout().flush();
            },
            options,
            Some(paste_rx),
        )
        .await?;

    install_oauth_tokens(&tokens).await?;
    mark_onboarding_complete();
    println!("Login successful.");
    Ok(())
}

/// Browser login for in-process / TUI use (no stdin paste thread).
pub async fn run_browser_login(
    login_with_claude_ai: bool,
    on_url: impl FnOnce(String) + Send,
    manual_rx: Option<mpsc::UnboundedReceiver<String>>,
) -> PortResult<OAuthTokens> {
    let oauth = OAuthService::new();
    let options = OAuthFlowOptions {
        login_with_claude_ai,
        ..Default::default()
    };
    let tokens = oauth
        .start_oauth_flow(
            |manual_url, _| async move {
                on_url(manual_url);
            },
            options,
            manual_rx,
        )
        .await?;
    install_oauth_tokens(&tokens).await?;
    mark_onboarding_complete();
    Ok(tokens)
}
