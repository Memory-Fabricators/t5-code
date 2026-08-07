//! Incremental tool registry — Read/Write/Edit/Glob/Grep/Bash/TodoWrite.

pub mod bash;
pub mod edit;
pub mod glob_tool;
pub mod grep_tool;
pub mod read;
pub mod state;
pub mod todo_write;
pub mod write;

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::constants::tool_names::{
    BASH_TOOL_NAME, FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME, GLOB_TOOL_NAME,
    GREP_TOOL_NAME, TODO_WRITE_TOOL_NAME,
};

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

/// Session state for the `-p` tool loop.
#[derive(Debug)]
pub struct ToolSession {
    pub read_file_state: ReadFileState,
    pub bash_cwd: PathBuf,
    pub todos: Vec<TodoItem>,
}

impl Default for ToolSession {
    fn default() -> Self {
        Self {
            read_file_state: ReadFileState::new(),
            bash_cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            todos: Vec::new(),
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
    ]
}

pub fn enabled_tool_names() -> Vec<String> {
    [
        BASH_TOOL_NAME,
        FILE_READ_TOOL_NAME,
        FILE_WRITE_TOOL_NAME,
        FILE_EDIT_TOOL_NAME,
        GLOB_TOOL_NAME,
        GREP_TOOL_NAME,
        TODO_WRITE_TOOL_NAME,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Dispatch a tool_use by name. Permissions are allow-all for `-p`.
pub async fn execute_tool(name: &str, input: &Value, session: &mut ToolSession) -> ToolResult {
    match name {
        FILE_READ_TOOL_NAME => read::execute(input, &mut session.read_file_state),
        FILE_WRITE_TOOL_NAME => write::execute(input, &mut session.read_file_state),
        FILE_EDIT_TOOL_NAME => edit::execute(input, &mut session.read_file_state),
        GLOB_TOOL_NAME => glob_tool::execute(input),
        GREP_TOOL_NAME => grep_tool::execute(input),
        BASH_TOOL_NAME => bash::execute(input, &mut session.bash_cwd).await,
        TODO_WRITE_TOOL_NAME => todo_write::execute(input, &mut session.todos),
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
