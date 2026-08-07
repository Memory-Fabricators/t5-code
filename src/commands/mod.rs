//! Slash commands — incremental ports of `src/commands/`.

pub mod model;
pub mod suggest;

pub use suggest::CommandSuggestions;

/// Result of running a slash command (shown in the REPL history, not sent to the API).
#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub message: String,
    /// When set, the REPL should switch the active conversation to this model ID.
    pub new_model: Option<String>,
    /// When set, update session effort (from ModelPicker).
    pub new_effort: Option<crate::utils::effort::EffortLevel>,
}

impl CommandOutcome {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            new_model: None,
            new_effort: None,
        }
    }
}

/// What a slash command wants the REPL to do.
#[derive(Debug, Clone)]
pub enum CommandAction {
    Message(CommandOutcome),
    /// Open the interactive ModelPicker overlay (`/model` with no args).
    OpenModelPicker,
}

/// Metadata for autocomplete + `/help`.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinCommand {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
}

pub fn builtin_commands() -> &'static [BuiltinCommand] {
    &[
        BuiltinCommand {
            name: "model",
            aliases: &[],
            description: "Set the AI model (opens picker)",
        },
        BuiltinCommand {
            name: "help",
            aliases: &[],
            description: "Show available commands",
        },
        BuiltinCommand {
            name: "exit",
            aliases: &["quit"],
            description: "Leave the REPL",
        },
    ]
}

/// Parse `/name args…` → `(name, args)`.
pub fn parse_slash(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let rest = trimmed[1..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let (name, args) = match rest.split_once(char::is_whitespace) {
        Some((n, a)) => (n, a.trim()),
        None => (rest, ""),
    };
    if name.is_empty() {
        None
    } else {
        Some((name, args))
    }
}

/// Dispatch a slash command by name.
pub async fn dispatch(
    name: &str,
    args: &str,
    current_model: &str,
) -> Result<CommandAction, String> {
    match name {
        "model" => Ok(model::run(args, current_model).await),
        "help" => Ok(CommandAction::Message(CommandOutcome::message(help_text()))),
        "exit" | "quit" => Ok(CommandAction::Message(CommandOutcome::message(
            "Use Esc or Ctrl+C to exit.",
        ))),
        other => Err(format!("Unknown command: /{other}. Try /help or /model.")),
    }
}

fn help_text() -> String {
    let mut lines = vec!["Available commands:".to_string()];
    for cmd in builtin_commands() {
        let alias = if cmd.aliases.is_empty() {
            String::new()
        } else {
            format!(" ({})", cmd.aliases.join(", "))
        };
        lines.push(format!("  /{}{alias} — {}", cmd.name, cmd.description));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_model_args() {
        assert_eq!(parse_slash("/model"), Some(("model", "")));
        assert_eq!(parse_slash("/model sonnet"), Some(("model", "sonnet")));
        assert_eq!(parse_slash("  /model  opus  "), Some(("model", "opus")));
        assert_eq!(parse_slash("hello"), None);
    }
}
