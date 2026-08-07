//! Minimal non-streaming Messages API client for `-p` print mode.

use serde::Deserialize;
use serde_json::json;

use crate::PortResult;
use crate::constants::oauth::{BASE_API_URL, OAUTH_BETA_HEADER};
use crate::constants::prompts::get_system_prompt;
use crate::constants::system::build_system_blocks;
use crate::constants::tool_names::default_enabled_tool_names;
use crate::utils::auth::{check_and_refresh_oauth_token_if_needed, is_claude_ai_subscriber};
use crate::utils::http::{get_auth_headers, get_user_agent};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const CLI_BETA: &str = "claude-code-20250219";

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorEnvelope {
    #[serde(default)]
    error: Option<ApiErrorBody>,
}

fn format_api_error(status: reqwest::StatusCode, text: &str) -> String {
    if let Ok(env) = serde_json::from_str::<ApiErrorEnvelope>(text) {
        if let Some(err) = env.error {
            let kind = err.kind.unwrap_or_default();
            let msg = err.message.unwrap_or_default();
            if !msg.is_empty() {
                return format!("Messages API {status}: {kind}: {msg}");
            }
        }
    }
    if let Ok(parsed) = serde_json::from_str::<MessagesResponse>(text) {
        if let Some(err) = parsed.error {
            let msg = err.message.or(err.kind).unwrap_or_default();
            if !msg.is_empty() {
                return format!("Messages API {status}: {msg}");
            }
        }
    }
    format!("Messages API {status}: {text}")
}

/// Send a single user message and return concatenated assistant text blocks.
pub async fn create_message(model: &str, prompt: &str, max_tokens: u32) -> PortResult<String> {
    let _ = check_and_refresh_oauth_token_if_needed(0, false).await;

    let auth = get_auth_headers();
    if let Some(err) = auth.error {
        return Err(rootcause::report!(err).into());
    }

    // System prompt: attribution + CLI prefix + core getSystemPrompt sections
    // (same order as TS claude.ts). OAuth requires attribution or the API
    // returns a misleading 429.
    let tool_names = default_enabled_tool_names();
    let core = get_system_prompt(&tool_names, model, &[]).await?;
    let system_blocks = build_system_blocks(prompt, true, &core);
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            { "role": "user", "content": prompt }
        ],
        "system": system_blocks,
    });

    let url = format!("{BASE_API_URL}/v1/messages");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("User-Agent", get_user_agent())
        .header("x-app", "cli");

    let mut betas: Vec<String> = Vec::new();
    if is_claude_ai_subscriber() {
        betas.push(CLI_BETA.into());
        betas.push(OAUTH_BETA_HEADER.into());
    }
    for (k, v) in &auth.headers {
        if k.eq_ignore_ascii_case("anthropic-beta") {
            if !betas.iter().any(|b| b == v) {
                betas.push(v.clone());
            }
        } else {
            req = req.header(k, v);
        }
    }
    if !betas.is_empty() {
        req = req.header("anthropic-beta", betas.join(","));
    }

    let response = req.json(&body).send().await?;
    let status = response.status();
    let text = response.text().await?;

    if !status.is_success() {
        return Err(rootcause::report!(format_api_error(status, &text)).into());
    }

    let parsed: MessagesResponse = match serde_json::from_str(&text) {
        Ok(p) => p,
        Err(e) => {
            return Err(
                rootcause::report!(format!("Failed to parse Messages response: {e}")).into(),
            );
        }
    };

    let assistant = parsed
        .content
        .into_iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(assistant)
}
