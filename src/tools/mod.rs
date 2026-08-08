//! Incremental tool registry — core coding + web + plan + ask-user tools.

pub mod ask_user_question;
pub mod bash;
pub mod edit;
pub mod enter_plan_mode;
pub mod exit_plan_mode;
pub mod glob_tool;
pub mod grep_tool;
pub mod interaction;
pub mod read;
pub mod state;
pub mod todo_write;
pub mod web_fetch;
pub mod web_search;
pub mod write;

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::constants::tool_names::{
    ASK_USER_QUESTION_TOOL_NAME, BASH_TOOL_NAME, ENTER_PLAN_MODE_TOOL_NAME,
    EXIT_PLAN_MODE_TOOL_NAME, FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME,
    GLOB_TOOL_NAME, GREP_TOOL_NAME, TODO_WRITE_TOOL_NAME, WEB_FETCH_TOOL_NAME,
    WEB_SEARCH_TOOL_NAME,
};
use crate::tools::interaction::InteractionBridge;

/// Per-path read cache used by Write/Edit read-before-write checks.
#[derive(Debug, Clone)]
pub struct ReadFileEntry {
    pub content: String,
    /// Floored mtime ms at last read/write.
    pub timestamp: u64,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

impl ReadFileEntry {
    pub fn is_partial_view(&self) -> bool {
        self.offset.is_some() || self.limit.is_some()
    }
}

pub type ReadFileState = HashMap<PathBuf, ReadFileEntry>;

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub content: String,
    pub status: String,
    pub active_form: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionMode {
    #[default]
    Default,
    Plan,
}

/// Session state for the tool loop.
#[derive(Debug)]
pub struct ToolSession {
    pub read_file_state: ReadFileState,
    pub bash_cwd: PathBuf,
    pub todos: Vec<TodoItem>,
    pub permission_mode: PermissionMode,
    pub pre_plan_mode: Option<PermissionMode>,
    pub plan_file_path: Option<PathBuf>,
    pub inject_plan_reminder: bool,
    pub interaction: Option<InteractionBridge>,
}

impl Default for ToolSession {
    fn default() -> Self {
        Self {
            read_file_state: ReadFileState::new(),
            bash_cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            todos: Vec::new(),
            permission_mode: PermissionMode::Default,
            pre_plan_mode: None,
            plan_file_path: None,
            inject_plan_reminder: false,
            interaction: None,
        }
    }
}

#[derive(Debug)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

/// Anthropic tool definitions for the Messages `tools` array.
pub fn api_tool_definitions() -> Vec<Value> {
    vec![
        bash::api_schema(),
        read::api_schema(),
        write::api_schema(),
        edit::api_schema(),
        glob_tool::api_schema(),
        grep_tool::api_schema(),
        todo_write::api_schema(),
        ask_user_question::api_schema(),
        enter_plan_mode::api_schema(),
        exit_plan_mode::api_schema(),
        web_fetch::api_schema(),
        web_search::api_schema(),
    ]
}

pub fn enabled_tool_names() -> Vec<String> {
    crate::constants::tool_names::default_enabled_tool_names()
}

fn plan_mode_blocks_mutation(
    session: &ToolSession,
    name: &str,
    input: &Value,
) -> Option<ToolResult> {
    if session.permission_mode != PermissionMode::Plan {
        return None;
    }
    let plan_path = session.plan_file_path.as_ref()?;
    match name {
        FILE_WRITE_TOOL_NAME | FILE_EDIT_TOOL_NAME => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let resolved = PathBuf::from(path);
            if resolved != *plan_path
                && !resolved
                    .canonicalize()
                    .ok()
                    .zip(plan_path.canonicalize().ok())
                    .is_some_and(|(a, b)| a == b)
            {
                return Some(ToolResult {
                    content: format!(
                        "Plan mode: can only edit the plan file ({}). Use ExitPlanMode after the plan is ready.",
                        plan_path.display()
                    ),
                    is_error: true,
                });
            }
            None
        }
        BASH_TOOL_NAME => Some(ToolResult {
            content: "Plan mode: Bash is disabled except for read-only exploration. Prefer Read/Glob/Grep, or ExitPlanMode after approval.".into(),
            is_error: true,
        }),
        _ => None,
    }
}

/// Dispatch a tool_use by name.
pub async fn execute_tool(name: &str, input: &Value, session: &mut ToolSession) -> ToolResult {
    if let Some(blocked) = plan_mode_blocks_mutation(session, name, input) {
        return blocked;
    }
    match name {
        FILE_READ_TOOL_NAME => read::execute(input, &mut session.read_file_state),
        FILE_WRITE_TOOL_NAME => write::execute(input, &mut session.read_file_state),
        FILE_EDIT_TOOL_NAME => edit::execute(input, &mut session.read_file_state),
        GLOB_TOOL_NAME => glob_tool::execute(input),
        GREP_TOOL_NAME => grep_tool::execute(input),
        BASH_TOOL_NAME => bash::execute(input, &mut session.bash_cwd).await,
        TODO_WRITE_TOOL_NAME => todo_write::execute(input, &mut session.todos),
        ASK_USER_QUESTION_TOOL_NAME => ask_user_question::execute(input, session).await,
        ENTER_PLAN_MODE_TOOL_NAME => enter_plan_mode::execute(session),
        EXIT_PLAN_MODE_TOOL_NAME => exit_plan_mode::execute(session).await,
        WEB_FETCH_TOOL_NAME => web_fetch::execute(input).await,
        WEB_SEARCH_TOOL_NAME => web_search::execute(input).await,
        other => ToolResult {
            content: format!("Unknown tool: {other}"),
            is_error: true,
        },
    }
}

pub fn tool_result_block(tool_use_id: &str, result: &ToolResult) -> Value {
    let mut block = json!({
        "type": "tool_result",
        "tool_use_id": tool_use_id,
        "content": result.content,
    });
    if result.is_error {
        block["is_error"] = json!(true);
    }
    block
}
