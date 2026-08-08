//! REPL application state.

use std::time::Instant;

use rand::RngExt as _;

use crate::commands::CommandSuggestions;
use crate::utils::effort::EffortLevel;

use super::model_picker::ModelPicker;
use super::theme::{SPINNER_FRAMES, SPINNER_VERBS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
    Error,
}

#[derive(Debug, Clone)]
pub struct ChatLine {
    pub role: Role,
    pub text: String,
}

#[derive(Debug)]
pub struct App {
    pub model: String,
    pub effort: Option<EffortLevel>,
    pub input: String,
    pub history: Vec<ChatLine>,
    pub busy: bool,
    pub spinner_verb: String,
    pub spinner_started: Instant,
    pub should_quit: bool,
    /// Lines scrolled up from the bottom of history.
    pub history_offset: usize,
    pub cursor_visible: bool,
    /// Interactive `/model` overlay (Ink ModelPicker).
    pub model_picker: Option<ModelPicker>,
    /// Slash-command autocomplete when input starts with `/`.
    pub suggestions: CommandSuggestions,
    /// Active AskUser / ExitPlan overlay.
    pub pending_interaction: Option<crate::tools::interaction::PendingUi>,
}

impl App {
    pub fn new(model: String) -> Self {
        Self {
            model,
            effort: crate::utils::effort::get_session_effort(),
            input: String::new(),
            history: Vec::new(),
            busy: false,
            spinner_verb: "Clauding".into(),
            spinner_started: Instant::now(),
            should_quit: false,
            history_offset: 0,
            cursor_visible: true,
            model_picker: None,
            suggestions: CommandSuggestions::default(),
            pending_interaction: None,
        }
    }

    pub fn open_model_picker(&mut self) {
        self.suggestions.clear();
        self.model_picker = Some(ModelPicker::open(&self.model));
    }

    pub fn set_input(&mut self, input: String) {
        self.input = input;
        self.refresh_suggestions();
    }

    pub fn push_input_char(&mut self, c: char) {
        self.input.push(c);
        self.refresh_suggestions();
    }

    pub fn pop_input_char(&mut self) {
        self.input.pop();
        self.refresh_suggestions();
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.suggestions.clear();
    }

    pub fn refresh_suggestions(&mut self) {
        if self.busy || self.model_picker.is_some() {
            self.suggestions.clear();
            return;
        }
        self.suggestions.refresh(&self.input);
    }

    pub fn accept_suggestion(&mut self) -> bool {
        if let Some(filled) = self.suggestions.accept() {
            self.set_input(filled);
            true
        } else {
            false
        }
    }

    pub fn push(&mut self, role: Role, text: impl Into<String>) {
        self.history.push(ChatLine {
            role,
            text: text.into(),
        });
        self.history_offset = 0;
    }

    pub fn start_busy(&mut self) {
        self.busy = true;
        self.suggestions.clear();
        self.spinner_started = Instant::now();
        let i = rand::rng().random_range(0..SPINNER_VERBS.len());
        self.spinner_verb = SPINNER_VERBS[i].to_string();
    }

    pub fn spinner_glyph(&self) -> &'static str {
        let i = (self.spinner_started.elapsed().as_millis() / 120) as usize % SPINNER_FRAMES.len();
        SPINNER_FRAMES[i]
    }

    pub fn placeholder(&self) -> &'static str {
        // Mirrors PromptInput placeholder flavor.
        "Try \"fix lint errors\" or ask a question…"
    }

    /// Effort shown in the footer (model default → high when unset).
    pub fn displayed_effort(&self) -> Option<EffortLevel> {
        if !crate::utils::effort::model_supports_effort(&self.model) {
            return None;
        }
        Some(crate::utils::effort::get_displayed_effort_level(
            &self.model,
            self.effort
                .or_else(crate::utils::effort::get_session_effort),
        ))
    }

    /// Cycle effort via Tab / Shift+Tab. Returns the new level if changed.
    pub fn cycle_effort(&mut self, dir: crate::utils::effort::CycleDir) -> Option<EffortLevel> {
        let next = crate::utils::effort::cycle_session_effort(&self.model, dir)?;
        self.effort = Some(next);
        Some(next)
    }

    /// Dim ghost suffix for the best prefix match (e.g. input `/mo` → `del`).
    pub fn ghost_suffix(&self) -> Option<String> {
        let item = self.suggestions.selected_item()?;
        let typed = self.input.trim_start();
        if !typed.starts_with('/') {
            return None;
        }
        let partial = &typed[1..];
        if partial.contains(char::is_whitespace) {
            return None;
        }
        let name = &item.name;
        if name.len() > partial.len()
            && name
                .to_ascii_lowercase()
                .starts_with(&partial.to_ascii_lowercase())
        {
            Some(name[partial.len()..].to_string())
        } else {
            None
        }
    }
}
