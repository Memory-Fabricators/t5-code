//! REPL application state.

use std::time::Instant;

use rand::RngExt as _;

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
    pub input: String,
    pub history: Vec<ChatLine>,
    pub busy: bool,
    pub spinner_verb: String,
    pub spinner_started: Instant,
    pub should_quit: bool,
    /// Lines scrolled up from the bottom of history.
    pub history_offset: usize,
    pub cursor_visible: bool,
}

impl App {
    pub fn new(model: String) -> Self {
        Self {
            model,
            input: String::new(),
            history: Vec::new(),
            busy: false,
            spinner_verb: "Clauding".into(),
            spinner_started: Instant::now(),
            should_quit: false,
            history_offset: 0,
            cursor_visible: true,
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
}
