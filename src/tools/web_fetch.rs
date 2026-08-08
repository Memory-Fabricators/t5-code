//! Port of WebFetchTool (MVP: fetch + HTML strip + optional Haiku summarize).

use serde_json::{Value, json};

use crate::constants::tool_names::WEB_FETCH_TOOL_NAME;
use crate::tools::ToolResult;
use crate::utils::http::{get_auth_headers, get_user_agent};

const MAX_MARKDOWN: usize = 100_000;
const SUMMARIZE_THRESHOLD: usize = 8_000;

pub fn api_schema() -> Value {
    json!({
        "name": WEB_FETCH_TOOL_NAME,
        "description": "Fetch a URL and extract its contents. Provide a prompt describing what to extract or summarize.",
        "input_schema": {
            "type": "object",
            "properties": {
                "url": { "type": "string", "format": "uri" },
                "prompt": { "type": "string" }
            },
            "required": ["url", "prompt"]
        }
    })
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let bytes = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    while i < bytes.len() {
        if !in_tag && lower[i..].starts_with("<script") {
            in_script = true;
        }
        if in_script && lower[i..].starts_with("</script>") {
            in_script = false;
            i += "</script>".len();
            continue;
        }
        if in_script {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            b if !in_tag => {
                out.push(b as char);
            }
            _ => {}
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn summarize_with_haiku(prompt: &str, content: &str) -> Result<String, String> {
    let model = "claude-haiku-4-5-20251001";
    let auth = get_auth_headers();
    if let Some(err) = auth.error {
        return Err(err);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client
        .post(format!(
            "{}/v1/messages",
            crate::constants::oauth::BASE_API_URL
        ))
        .header("anthropic-version", "2023-06-01")
        .header("User-Agent", get_user_agent())
        .json(&json!({
            "model": model,
            "max_tokens": 2048,
            "messages": [{
                "role": "user",
                "content": format!(
                    "Based on the following web page content, answer this prompt:\n\n{prompt}\n\n---\n\n{content}"
                )
            }]
        }));
    for (k, v) in auth.headers {
        req = req.header(k, v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("summarize failed: {status} {text}"));
    }
    let parsed: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let summary = parsed
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    Ok(summary)
}

pub async fn execute(input: &Value) -> ToolResult {
    let url = input
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("Summarize this page.")
        .to_string();

    if url.is_empty() || (!url.starts_with("http://") && !url.starts_with("https://")) {
        return ToolResult {
            content: "WebFetch requires a valid http(s) URL".into(),
            is_error: true,
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                content: e.to_string(),
                is_error: true,
            };
        }
    };

    let response = match client
        .get(&url)
        .header("User-Agent", get_user_agent())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return ToolResult {
                content: format!("Fetch failed: {e}"),
                is_error: true,
            };
        }
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            return ToolResult {
                content: format!("Read body failed: {e}"),
                is_error: true,
            };
        }
    };

    if !status.is_success() {
        return ToolResult {
            content: format!("HTTP {status} fetching {url}"),
            is_error: true,
        };
    }

    let mut markdown = if body.trim_start().starts_with('<') {
        html_to_text(&body)
    } else {
        body
    };
    if markdown.len() > MAX_MARKDOWN {
        markdown.truncate(MAX_MARKDOWN);
    }

    let result = if markdown.len() > SUMMARIZE_THRESHOLD {
        match summarize_with_haiku(&prompt, &markdown).await {
            Ok(s) => s,
            Err(e) => format!(
                "(summarize failed: {e})\n\nFirst part of content:\n{}",
                &markdown[..markdown.len().min(4000)]
            ),
        }
    } else {
        format!("Prompt: {prompt}\n\nContent:\n{markdown}")
    };

    ToolResult {
        content: result,
        is_error: false,
    }
}
