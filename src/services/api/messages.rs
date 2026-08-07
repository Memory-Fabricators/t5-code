//! Minimal non-streaming Messages API client for `-p` print mode (with tool loop).

use serde::Deserialize;
use serde_json::{Value, json};

use crate::PortResult;
use crate::constants::oauth::{BASE_API_URL, OAUTH_BETA_HEADER};
use crate::constants::prompts::get_system_prompt;
use crate::constants::system::build_system_blocks;
use crate::tools::{
    ReadFileState, api_tool_definitions, enabled_tool_names, execute_tool, tool_result_block,
};
use crate::utils::auth::{check_and_refresh_oauth_token_if_needed, is_claude_ai_subscriber};
use crate::utils::http::{get_auth_headers, get_user_agent};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const CLI_BETA: &str = "claude-code-20250219";
const MAX_TOOL_TURNS: usize = 20;

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    #[allow(dead_code)]
    stop_reason: Option<String>,
    #[serde(default)]
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Deserialize, Clone)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
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

fn content_blocks_to_json(blocks: &[ContentBlock]) -> Value {
    let arr: Vec<Value> = blocks
        .iter()
        .map(|b| match b.kind.as_str() {
            "text" => json!({
                "type": "text",
                "text": b.text.clone().unwrap_or_default(),
            }),
            "tool_use" => json!({
                "type": "tool_use",
                "id": b.id.clone().unwrap_or_default(),
                "name": b.name.clone().unwrap_or_default(),
                "input": b.input.clone().unwrap_or(json!({})),
            }),
            other => json!({ "type": other }),
        })
        .collect();
    Value::Array(arr)
}

fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter(|b| b.kind == "text")
        .filter_map(|b| b.text.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join("")
}

async fn create_message_turn(
    model: &str,
    system_blocks: &[Value],
    messages: &Value,
    tools: &[Value],
    max_tokens: u32,
) -> PortResult<MessagesResponse> {
    let auth = get_auth_headers();
    if let Some(err) = auth.error {
        return Err(rootcause::report!(err).into());
    }

    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
        "system": system_blocks,
        "tools": tools,
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

    match serde_json::from_str::<MessagesResponse>(&text) {
        Ok(p) => Ok(p),
        Err(e) => Err(rootcause::report!(format!("Failed to parse Messages response: {e}")).into()),
    }
}

/// Run a `-p` query with Read/Write tools until the model stops calling tools.
pub async fn create_message(model: &str, prompt: &str, max_tokens: u32) -> PortResult<String> {
    let _ = check_and_refresh_oauth_token_if_needed(0, false).await;

    let tool_names = enabled_tool_names();
    let tools = api_tool_definitions();
    let core = get_system_prompt(&tool_names, model, &[]).await?;
    let system_blocks = build_system_blocks(prompt, true, &core);

    let mut messages = json!([
        { "role": "user", "content": prompt }
    ]);
    let mut state = ReadFileState::new();
    let mut final_text = String::new();

    for _ in 0..MAX_TOOL_TURNS {
        let resp =
            create_message_turn(model, &system_blocks, &messages, &tools, max_tokens).await?;

        let tool_uses: Vec<&ContentBlock> = resp
            .content
            .iter()
            .filter(|b| b.kind == "tool_use")
            .collect();

        let text = extract_text(&resp.content);
        if !text.is_empty() {
            final_text = text;
        }

        // Append assistant turn (full content blocks for pairing).
        if let Some(arr) = messages.as_array_mut() {
            arr.push(json!({
                "role": "assistant",
                "content": content_blocks_to_json(&resp.content),
            }));
        }

        if tool_uses.is_empty() {
            return Ok(final_text);
        }

        let empty = json!({});
        let mut results = Vec::new();
        for tu in &tool_uses {
            let id = tu.id.as_deref().unwrap_or("");
            let name = tu.name.as_deref().unwrap_or("");
            let input = tu.input.as_ref().unwrap_or(&empty);
            let result = execute_tool(name, input, &mut state);
            results.push(tool_result_block(id, &result));
        }

        if let Some(arr) = messages.as_array_mut() {
            arr.push(json!({
                "role": "user",
                "content": results,
            }));
        }
    }

    Err(rootcause::report!(format!("Exceeded maximum tool turns ({MAX_TOOL_TURNS})")).into())
}
