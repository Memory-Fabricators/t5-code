//! Port of `src/constants/system.ts` — attribution + CLI sysprompt prefix.

use crate::utils::fingerprint::compute_fingerprint;

const AGENT_SDK_PREFIX: &str = "You are a Claude agent, built on Anthropic's Claude Agent SDK.";
const DEFAULT_PREFIX: &str = "You are Claude Code, Anthropic's official CLI for Claude.";

/// CLI system prompt prefix. Non-interactive print mode uses the Agent SDK prefix.
pub fn get_cli_sysprompt_prefix(is_non_interactive: bool) -> &'static str {
    if is_non_interactive {
        AGENT_SDK_PREFIX
    } else {
        DEFAULT_PREFIX
    }
}

/// `x-anthropic-billing-header: cc_version=…; cc_entrypoint=…;`
pub fn get_attribution_header(fingerprint: &str) -> String {
    if matches!(
        std::env::var("CLAUDE_CODE_ATTRIBUTION_HEADER")
            .ok()
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("0" | "false" | "no" | "off")
    ) {
        return String::new();
    }

    let version = format!("{}.{}", env!("CARGO_PKG_VERSION"), fingerprint);
    let entrypoint = std::env::var("CLAUDE_CODE_ENTRYPOINT").unwrap_or_else(|_| "cli".into());
    format!("x-anthropic-billing-header: cc_version={version}; cc_entrypoint={entrypoint};")
}

/// Build API `system` blocks matching TS order in `claude.ts`:
/// attribution → CLI prefix → core `getSystemPrompt` sections.
pub fn build_system_blocks(
    first_user_message: &str,
    is_non_interactive: bool,
    core_sections: &[String],
) -> Vec<serde_json::Value> {
    let fp = compute_fingerprint(first_user_message, env!("CARGO_PKG_VERSION"));
    let mut blocks = Vec::new();
    let attr = get_attribution_header(&fp);
    if !attr.is_empty() {
        blocks.push(serde_json::json!({ "type": "text", "text": attr }));
    }
    blocks.push(serde_json::json!({
        "type": "text",
        "text": get_cli_sysprompt_prefix(is_non_interactive),
    }));
    for section in core_sections {
        if !section.is_empty() {
            blocks.push(serde_json::json!({ "type": "text", "text": section }));
        }
    }
    blocks
}

/// OAuth-only helper kept for callers that only need attr + prefix.
pub fn build_oauth_system_blocks(
    first_user_message: &str,
    is_non_interactive: bool,
) -> Vec<serde_json::Value> {
    build_system_blocks(first_user_message, is_non_interactive, &[])
}
