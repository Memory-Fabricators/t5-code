//! Port of `src/commands/model/` — `/model` slash command.

use crate::commands::{CommandAction, CommandOutcome};
use crate::utils::model::{
    MODEL_ALIASES, get_default_main_loop_model, is_model_alias, parse_user_specified_model,
    render_model_label, render_model_name, set_session_model,
};

const HELP_ARGS: &[&str] = &["help", "-h", "--help"];
const INFO_ARGS: &[&str] = &[
    "list", "show", "display", "current", "view", "get", "check", "describe", "print", "version",
    "about", "status", "?",
];

/// Run `/model [args]`. Empty args open the interactive picker in the REPL.
pub async fn run(args: &str, current_model: &str) -> CommandAction {
    let args = args.trim();

    if HELP_ARGS.contains(&args) {
        return CommandAction::Message(CommandOutcome {
            message: "Run /model to open the model selection menu, or /model [modelName] to set the model.\n\
Aliases: sonnet, opus, haiku, best, sonnet[1m], opus[1m], opusplan, default"
                .into(),
            new_model: None,
            new_effort: None,
        });
    }

    if INFO_ARGS.contains(&args) {
        return CommandAction::Message(CommandOutcome {
            message: format_status(current_model),
            new_model: None,
            new_effort: None,
        });
    }

    // No args → interactive ModelPicker (Ink JSX path).
    if args.is_empty() {
        return CommandAction::OpenModelPicker;
    }

    // `/model default` → built-in default
    if args.eq_ignore_ascii_case("default") {
        set_session_model(None);
        let resolved = get_default_main_loop_model();
        return CommandAction::Message(CommandOutcome {
            message: format!("Set model to {}", render_model_label(None)),
            new_model: Some(resolved),
            new_effort: None,
        });
    }

    // Known aliases skip remote validation.
    if is_model_alias(args) || is_model_alias(&args.to_ascii_lowercase()) {
        let resolved = parse_user_specified_model(args);
        set_session_model(Some(args.to_string()));
        return CommandAction::Message(CommandOutcome {
            message: format!("Set model to {}", render_model_name(args)),
            new_model: Some(resolved),
            new_effort: None,
        });
    }

    // Custom / full model id — accept as-is (API validates on next turn).
    let resolved = parse_user_specified_model(args);
    set_session_model(Some(args.to_string()));
    CommandAction::Message(CommandOutcome {
        message: format!("Set model to {}", render_model_name(args)),
        new_model: Some(resolved),
        new_effort: None,
    })
}

fn format_status(current_model: &str) -> String {
    let mut lines = vec![
        format!("Current model: {}", render_model_name(current_model)),
        String::new(),
        "Aliases:".into(),
    ];
    for alias in MODEL_ALIASES {
        let resolved = parse_user_specified_model(alias);
        lines.push(format!("  {alias:<12} → {}", render_model_name(&resolved)));
    }
    lines.push("  default      → reset to built-in default".into());
    lines.push(String::new());
    lines.push("Usage: /model  (opens picker)  ·  /model [alias|model-id]".into());
    lines.join("\n")
}
