//! Interactive tool prompts (AskUserQuestion, ExitPlanMode approval).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone)]
pub enum UserPrompt {
    AskUser { questions: Vec<Question> },
    ExitPlan { plan: String, path: String },
}

#[derive(Debug, Clone)]
pub enum UserResponse {
    AskUser { answers: HashMap<String, String> },
    ExitPlan { approved: bool },
    Cancelled,
}

pub struct InteractionRequest {
    pub prompt: UserPrompt,
    pub response_tx: oneshot::Sender<UserResponse>,
}

/// Bridge from the tool loop → TUI. Cloned into `ToolSession`.
#[derive(Clone)]
pub struct InteractionBridge {
    tx: mpsc::UnboundedSender<InteractionRequest>,
}

impl std::fmt::Debug for InteractionBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InteractionBridge")
    }
}

impl InteractionBridge {
    pub fn pair() -> (Self, mpsc::UnboundedReceiver<InteractionRequest>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub async fn ask(&self, prompt: UserPrompt) -> UserResponse {
        let (response_tx, response_rx) = oneshot::channel();
        if self
            .tx
            .send(InteractionRequest {
                prompt,
                response_tx,
            })
            .is_err()
        {
            return UserResponse::Cancelled;
        }
        response_rx.await.unwrap_or(UserResponse::Cancelled)
    }
}

/// Shared pending interaction for the draw loop (optional overlay state).
pub type PendingInteraction = Arc<Mutex<Option<PendingUi>>>;

pub struct PendingUi {
    pub prompt: UserPrompt,
    pub response_tx: Option<oneshot::Sender<UserResponse>>,
    /// AskUser: which question index is active.
    pub question_idx: usize,
    /// AskUser: selected option indices for current question (multi).
    pub selected: Vec<usize>,
    /// AskUser: collected answers so far.
    pub answers: HashMap<String, String>,
    /// AskUser: free-text "Other" buffer.
    pub other_text: String,
    /// AskUser: editing other text.
    pub editing_other: bool,
    /// ExitPlan: cursor on Approve (0) or Reject (1).
    pub exit_choice: usize,
}

impl std::fmt::Debug for PendingUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingUi")
            .field("prompt", &self.prompt)
            .field("question_idx", &self.question_idx)
            .field("selected", &self.selected)
            .field("answers", &self.answers)
            .field("other_text", &self.other_text)
            .field("editing_other", &self.editing_other)
            .field("exit_choice", &self.exit_choice)
            .finish()
    }
}

impl PendingUi {
    pub fn from_request(req: InteractionRequest) -> Self {
        Self {
            prompt: req.prompt,
            response_tx: Some(req.response_tx),
            question_idx: 0,
            selected: Vec::new(),
            answers: HashMap::new(),
            other_text: String::new(),
            editing_other: false,
            exit_choice: 0,
        }
    }
}
