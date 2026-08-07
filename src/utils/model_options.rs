//! Port of `src/utils/model/modelOptions.ts` — options shown in `/model` picker.
//!
//! Simplified to the first-party PAYG list (Default + Sonnet 1M + Opus + Opus 1M + Haiku).
//! Subscriber-tier branching and allowlist filtering can land later.

use crate::utils::model::{get_default_main_loop_model, get_session_model, render_model_name};

/// Sentinel value for the "Default (recommended)" row (`null` in TS).
pub const NO_PREFERENCE: &str = "__NO_PREFERENCE__";

#[derive(Debug, Clone)]
pub struct ModelOption {
    /// Alias / model id, or [`NO_PREFERENCE`] for default.
    pub value: String,
    pub label: String,
    pub description: String,
}

/// Build the picker list, ensuring the current selection is present.
pub fn get_model_options(current: &str) -> Vec<ModelOption> {
    let mut options = base_options();

    // Ensure current selection appears (edge: custom / full model id).
    let current_key = selection_key(current);
    if !options.iter().any(|o| o.value == current_key) {
        options.push(ModelOption {
            value: current.to_string(),
            label: render_model_name(current),
            description: "Current model".into(),
        });
    }

    // Env custom option.
    if let Ok(custom) = std::env::var("ANTHROPIC_CUSTOM_MODEL_OPTION")
        && !custom.is_empty()
        && !options.iter().any(|o| o.value == custom)
    {
        let label =
            std::env::var("ANTHROPIC_CUSTOM_MODEL_OPTION_NAME").unwrap_or_else(|_| custom.clone());
        let description = std::env::var("ANTHROPIC_CUSTOM_MODEL_OPTION_DESCRIPTION")
            .unwrap_or_else(|_| format!("Custom model ({custom})"));
        options.push(ModelOption {
            value: custom,
            label,
            description,
        });
    }

    options
}

fn base_options() -> Vec<ModelOption> {
    let default_name = render_model_name(&get_default_main_loop_model());
    vec![
        ModelOption {
            value: NO_PREFERENCE.into(),
            label: "Default (recommended)".into(),
            description: format!("Use the default model (currently {default_name})"),
        },
        ModelOption {
            value: "sonnet[1m]".into(),
            label: "Sonnet (1M context)".into(),
            description: "Sonnet 4.6 for long sessions".into(),
        },
        ModelOption {
            value: "opus".into(),
            label: "Opus".into(),
            description: "Opus 4.6 · Most capable for complex work".into(),
        },
        ModelOption {
            value: "opus[1m]".into(),
            label: "Opus (1M context)".into(),
            description: "Opus 4.6 for long sessions".into(),
        },
        ModelOption {
            value: "haiku".into(),
            label: "Haiku".into(),
            description: "Haiku 4.5 · Fastest for quick answers".into(),
        },
        // Also surface plain Sonnet as an explicit choice (matches Max tier lists).
        ModelOption {
            value: "sonnet".into(),
            label: "Sonnet".into(),
            description: "Sonnet 4.6 · Best for everyday tasks".into(),
        },
    ]
}

/// Map the live model id / session setting onto a picker row value.
pub fn selection_key(current_model: &str) -> String {
    if get_session_model().is_none() {
        // No session override — treat as Default when it matches the built-in default.
        let def = get_default_main_loop_model();
        if models_equal(current_model, &def) {
            return NO_PREFERENCE.into();
        }
    }
    if let Some(session) = get_session_model() {
        // Prefer the alias the user set.
        return session;
    }
    current_model.to_string()
}

fn models_equal(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
        || crate::utils::model::parse_user_specified_model(a)
            .eq_ignore_ascii_case(&crate::utils::model::parse_user_specified_model(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::model::set_session_model;

    #[test]
    fn options_include_default() {
        set_session_model(None);
        let opts = get_model_options("claude-sonnet-4-6");
        assert!(opts.iter().any(|o| o.value == NO_PREFERENCE));
        assert!(opts.len() >= 5);
    }
}
