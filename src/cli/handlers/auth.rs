//! Port of `src/cli/handlers/auth.ts`.

use rootcause::Report;

use crate::PortResult;
use crate::services::oauth::client::{populate_oauth_account_info_if_needed, refresh_oauth_token};
use crate::utils::auth::{
    AuthTokenSource, auth_status_json, check_and_refresh_oauth_token_if_needed,
    get_auth_token_source, perform_logout, save_oauth_tokens_if_needed,
};
use crate::utils::config::save_global_config;

fn report_msg(msg: impl Into<String>) -> Report {
    rootcause::report!(msg.into()).into()
}

pub async fn auth_status(json: bool) -> PortResult<()> {
    // Refresh expired tokens before reporting status.
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

        let _ = perform_logout();
        save_oauth_tokens_if_needed(&tokens).map_err(report_msg)?;
        let _ = populate_oauth_account_info_if_needed().await;

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

        println!("Login successful.");
        return Ok(());
    }

    eprintln!(
        "Browser OAuth is not ported yet.\n\
         Options:\n\
         1. Use `claude auth login` then `t5-code auth status`\n\
         2. Set CLAUDE_CODE_OAUTH_REFRESH_TOKEN and CLAUDE_CODE_OAUTH_SCOPES, then re-run `t5-code auth login`"
    );
    std::process::exit(1);
}
