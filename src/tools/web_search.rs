//! Port of WebSearchTool via nested Messages `web_search_20250305`.

use serde_json::{Value, json};

use crate::constants::oauth::{BASE_API_URL, OAUTH_BETA_HEADER};
use crate::constants::tool_names::WEB_SEARCH_TOOL_NAME;
use crate::tools::ToolResult;
use crate::utils::auth::is_claude_ai_subscriber;
use crate::utils::http::{get_auth_headers, get_user_agent};

pub fn api_schema() -> Value {
    json!({
        "name": WEB_SEARCH_TOOL_NAME,
        "description": "Search the web for current information. Returns titles, URLs, and snippets.",
        "input_schema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "minLength": 2 },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["query"]
        }
    })
}

pub async fn execute(input: &Value) -> ToolResult {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if query.len() < 2 {
        return ToolResult {
            content: "WebSearch query must be at least 2 characters".into(),
            is_error: true,
        };
    }

    let mut server_tool = json!({
        "type": "web_search_20250305",
        "name": "web_search",
        "max_uses": 8
    });
    if let Some(domains) = input.get("allowed_domains") {
        server_tool["allowed_domains"] = domains.clone();
    }
    if let Some(domains) = input.get("blocked_domains") {
        server_tool["blocked_domains"] = domains.clone();
    }

    let auth = get_auth_headers();
    if let Some(err) = auth.error {
        return ToolResult {
            content: err,
            is_error: true,
        };
    }

    let model = "claude-haiku-4-5-20251001";
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
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

    let mut betas = vec!["claude-code-20250219".to_string()];
    if is_claude_ai_subscriber() {
        betas.push(OAUTH_BETA_HEADER.to_string());
    }

    let mut req = client
        .post(format!("{BASE_API_URL}/v1/messages"))
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", betas.join(","))
        .header("User-Agent", get_user_agent())
        .json(&json!({
            "model": model,
            "max_tokens": 2048,
            "tools": [server_tool],
            "messages": [{
                "role": "user",
                "content": format!("Search the web for: {query}")
            }]
        }));
    for (k, v) in auth.headers {
        req = req.header(k, v);
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return ToolResult {
                content: format!("WebSearch request failed: {e}"),
                is_error: true,
            };
        }
    };
    let status = resp.status();
    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            return ToolResult {
                content: e.to_string(),
                is_error: true,
            };
        }
    };
    if !status.is_success() {
        return ToolResult {
            content: format!("WebSearch API {status}: {text}"),
            is_error: true,
        };
    }

    let parsed: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return ToolResult {
                content: format!("Failed to parse WebSearch response: {e}"),
                is_error: true,
            };
        }
    };

    let mut text_parts = Vec::new();
    let mut links: Vec<Value> = Vec::new();

    if let Some(content) = parsed.get("content").and_then(|c| c.as_array()) {
        for block in content {
            let kind = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match kind {
                "text" => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(t.to_string());
                    }
                }
                "web_search_tool_result" => {
                    if let Some(items) = block.get("content").and_then(|c| c.as_array()) {
                        for item in items {
                            if item.get("type").and_then(|t| t.as_str())
                                == Some("web_search_result")
                            {
                                let title =
                                    item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                                let url = item.get("url").and_then(|t| t.as_str()).unwrap_or("");
                                links.push(json!({ "title": title, "url": url }));
                                if let Some(snippet) =
                                    item.get("encrypted_content").and_then(|t| t.as_str())
                                {
                                    let _ = snippet; // encrypted; use title/url
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut out = format!("Web search results for query: \"{query}\"\n\n");
    if !text_parts.is_empty() {
        out.push_str(&text_parts.join("\n"));
        out.push_str("\n\n");
    }
    if !links.is_empty() {
        out.push_str("Links: ");
        out.push_str(&serde_json::to_string(&links).unwrap_or_else(|_| "[]".into()));
        out.push('\n');
    }
    out.push_str(
        "\nREMINDER: You must cite sources using markdown hyperlinks in your final response.",
    );

    ToolResult {
        content: out,
        is_error: false,
    }
}
