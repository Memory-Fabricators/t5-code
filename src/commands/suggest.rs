//! Slash command autocomplete — simplified port of `utils/suggestions/commandSuggestions.ts`.

use crate::commands::builtin_commands;

#[derive(Debug, Clone)]
pub struct SuggestionItem {
    pub name: String,
    pub display: String,
    pub description: String,
}

#[derive(Debug, Clone, Default)]
pub struct CommandSuggestions {
    pub items: Vec<SuggestionItem>,
    pub selected: usize,
}

impl CommandSuggestions {
    pub fn clear(&mut self) {
        self.items.clear();
        self.selected = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn selected_item(&self) -> Option<&SuggestionItem> {
        self.items.get(self.selected)
    }

    pub fn move_up(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.items.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.items.len();
    }

    /// Refresh from the current prompt input. Returns true if suggestions changed.
    pub fn refresh(&mut self, input: &str) {
        let next = generate(input);
        let keep = self
            .selected_item()
            .and_then(|cur| next.iter().position(|n| n.name == cur.name))
            .unwrap_or(0);
        self.items = next;
        self.selected = if self.items.is_empty() {
            0
        } else {
            keep.min(self.items.len() - 1)
        };
    }

    /// Accept selected → `/name` (no trailing space so Enter can run immediately).
    pub fn accept(&self) -> Option<String> {
        self.selected_item().map(|item| format!("/{}", item.name))
    }
}

/// Prefix-match suggestions for `/…` input with no arguments yet.
pub fn generate(input: &str) -> Vec<SuggestionItem> {
    let trimmed = input.trim_end();
    if !trimmed.starts_with('/') {
        return Vec::new();
    }
    // Arguments present → no suggestions (matches hasCommandArgs).
    if let Some((_, rest)) = trimmed.split_once(char::is_whitespace)
        && !rest.is_empty()
    {
        return Vec::new();
    }

    let query = trimmed[1..].to_ascii_lowercase();
    let mut out = Vec::new();

    for cmd in builtin_commands() {
        let name_l = cmd.name.to_ascii_lowercase();
        let alias_hit = cmd
            .aliases
            .iter()
            .find(|a| a.to_ascii_lowercase().starts_with(&query));

        let name_match = query.is_empty() || name_l.starts_with(&query);
        let alias_match = alias_hit.is_some();

        if !name_match && !alias_match {
            continue;
        }

        // Prefer showing the real command name; note matched alias if used.
        let display = if let Some(alias) = alias_hit.filter(|_| !name_match || !query.is_empty()) {
            if !name_l.starts_with(&query) {
                format!("/{} ({alias})", cmd.name)
            } else {
                format!("/{}", cmd.name)
            }
        } else {
            format!("/{}", cmd.name)
        };

        out.push(SuggestionItem {
            name: cmd.name.to_string(),
            display,
            description: cmd.description.to_string(),
        });
    }

    // Stable order: exact prefix length then alphabetical.
    out.sort_by(|a, b| {
        let a_exact = a.name.eq_ignore_ascii_case(&query);
        let b_exact = b.name.eq_ignore_ascii_case(&query);
        b_exact
            .cmp(&a_exact)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.name.cmp(&b.name))
    });

    out.truncate(8);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_on_slash() {
        let s = generate("/");
        assert!(s.iter().any(|i| i.name == "model"));
        assert!(s.iter().any(|i| i.name == "help"));
    }

    #[test]
    fn filters_prefix() {
        let s = generate("/mo");
        assert!(
            s.iter()
                .all(|i| i.name.starts_with("mo") || i.name == "model")
        );
        assert!(s.iter().any(|i| i.name == "model"));
    }

    #[test]
    fn no_suggest_with_args() {
        assert!(generate("/model sonnet").is_empty());
    }

    #[test]
    fn quit_alias() {
        let s = generate("/qui");
        assert!(s.iter().any(|i| i.name == "exit"));
    }
}
