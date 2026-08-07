//! Port of `src/utils/model/model.ts` — aliases, resolution, display names.

use std::sync::Mutex;

/// Default Sonnet model for Pro / standard subscribers.
pub const DEFAULT_SONNET_MODEL: &str = "claude-sonnet-4-6";
pub const DEFAULT_OPUS_MODEL: &str = "claude-opus-4-6";
pub const DEFAULT_HAIKU_MODEL: &str = "claude-haiku-4-5-20251001";

pub const MODEL_ALIASES: &[&str] = &[
    "sonnet",
    "opus",
    "haiku",
    "best",
    "sonnet[1m]",
    "opus[1m]",
    "opusplan",
];

/// Session override from `/model` (highest priority within the process).
static SESSION_MODEL: Mutex<Option<String>> = Mutex::new(None);

pub fn get_default_sonnet_model() -> String {
    std::env::var("ANTHROPIC_DEFAULT_SONNET_MODEL").unwrap_or_else(|_| DEFAULT_SONNET_MODEL.into())
}

pub fn get_default_opus_model() -> String {
    std::env::var("ANTHROPIC_DEFAULT_OPUS_MODEL").unwrap_or_else(|_| DEFAULT_OPUS_MODEL.into())
}

pub fn get_default_haiku_model() -> String {
    std::env::var("ANTHROPIC_DEFAULT_HAIKU_MODEL").unwrap_or_else(|_| DEFAULT_HAIKU_MODEL.into())
}

pub fn get_best_model() -> String {
    get_default_opus_model()
}

/// Built-in default when the user has not specified a model.
pub fn get_default_main_loop_model() -> String {
    get_default_sonnet_model()
}

pub fn set_session_model(model: Option<String>) {
    if let Ok(mut g) = SESSION_MODEL.lock() {
        *g = model;
    }
}

pub fn get_session_model() -> Option<String> {
    SESSION_MODEL.lock().ok().and_then(|g| g.clone())
}

/// Main-loop model: session `/model` → CLI/env → built-in default.
pub fn get_main_loop_model() -> String {
    if let Some(m) = get_session_model() {
        return parse_user_specified_model(&m);
    }
    if let Ok(m) = std::env::var("ANTHROPIC_MODEL")
        && !m.is_empty()
    {
        return parse_user_specified_model(&m);
    }
    get_default_main_loop_model()
}

pub fn resolve_model(cli_override: Option<&str>) -> String {
    if let Some(m) = get_session_model() {
        return parse_user_specified_model(&m);
    }
    cli_override
        .filter(|s| !s.is_empty())
        .map(parse_user_specified_model)
        .unwrap_or_else(get_main_loop_model)
}

pub fn is_model_alias(input: &str) -> bool {
    let n = input.to_ascii_lowercase();
    MODEL_ALIASES.iter().any(|a| *a == n)
}

/// Resolve a user-specified model alias or ID (port of `parseUserSpecifiedModel`).
pub fn parse_user_specified_model(model_input: &str) -> String {
    let trimmed = model_input.trim();
    let normalized = trimmed.to_ascii_lowercase();
    let has_1m = normalized.contains("[1m]");
    let base = normalized.replace("[1m]", "").trim().to_string();
    let suffix = if has_1m { "[1m]" } else { "" };

    match base.as_str() {
        "opusplan" => format!("{}{suffix}", get_default_sonnet_model()),
        "sonnet" => format!("{}{suffix}", get_default_sonnet_model()),
        "haiku" => format!("{}{suffix}", get_default_haiku_model()),
        "opus" => format!("{}{suffix}", get_default_opus_model()),
        "best" => get_best_model(),
        _ => {
            if has_1m {
                format!(
                    "{}[1m]",
                    trimmed.replace("[1m]", "").replace("[1M]", "").trim()
                )
            } else {
                trimmed.to_string()
            }
        }
    }
}

/// Marketing / display name for a model setting (alias or id).
pub fn render_model_name(model: &str) -> String {
    let resolved = parse_user_specified_model(model);
    if let Some(name) = marketing_name(&resolved) {
        if model != resolved && is_model_alias(&model.to_ascii_lowercase()) {
            return format!("{model} ({name})");
        }
        return name.to_string();
    }
    if model != resolved {
        format!("{model} ({resolved})")
    } else {
        resolved
    }
}

pub fn render_model_label(model: Option<&str>) -> String {
    match model {
        None => {
            let d = get_default_main_loop_model();
            format!("{} (default)", render_model_name(&d))
        }
        Some(m) => render_model_name(m),
    }
}

fn marketing_name(model_id: &str) -> Option<&'static str> {
    let has_1m = model_id.to_ascii_lowercase().contains("[1m]");
    let canonical = model_id.to_ascii_lowercase();
    if canonical.contains("claude-opus-4-6") {
        return Some(if has_1m {
            "Opus 4.6 (with 1M context)"
        } else {
            "Opus 4.6"
        });
    }
    if canonical.contains("claude-opus-4-5") {
        return Some("Opus 4.5");
    }
    if canonical.contains("claude-sonnet-4-6") {
        return Some(if has_1m {
            "Sonnet 4.6 (with 1M context)"
        } else {
            "Sonnet 4.6"
        });
    }
    if canonical.contains("claude-sonnet-4-5") {
        return Some(if has_1m {
            "Sonnet 4.5 (with 1M context)"
        } else {
            "Sonnet 4.5"
        });
    }
    if canonical.contains("claude-haiku-4-5") {
        return Some("Haiku 4.5");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_resolve() {
        assert!(parse_user_specified_model("sonnet").contains("sonnet"));
        assert!(parse_user_specified_model("opus").contains("opus"));
        assert!(parse_user_specified_model("haiku").contains("haiku"));
        assert_eq!(parse_user_specified_model("best"), get_default_opus_model());
    }

    #[tokio::test]
    async fn model_command_sets_session() {
        set_session_model(None);
        let out = crate::commands::model::run("sonnet", "claude-sonnet-4-6").await;
        match out {
            crate::commands::CommandAction::Message(o) => {
                assert!(o.message.contains("Set model"));
                assert!(o.new_model.unwrap().contains("sonnet"));
            }
            other => panic!("expected Message, got {other:?}"),
        }
        set_session_model(None);
    }

    #[tokio::test]
    async fn model_no_args_opens_picker() {
        let out = crate::commands::model::run("", "claude-sonnet-4-6").await;
        assert!(matches!(
            out,
            crate::commands::CommandAction::OpenModelPicker
        ));
    }
}
