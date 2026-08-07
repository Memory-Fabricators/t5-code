//! Incremental tool registry — Read/Write first (TS `src/tools/`).

pub mod read;
pub mod write;

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::constants::tool_names::{FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME};

/// Per-path read cache used by Write's read-before-write check.
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

#[derive(Debug)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

/// Anthropic tool definitions for the Messages `tools` array.
pub fn api_tool_definitions() -> Vec<Value> {
    vec![read::api_schema(), write::api_schema()]
}

pub fn enabled_tool_names() -> Vec<String> {
    [FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Dispatch a tool_use by name. Permissions are allow-all for `-p`.
pub fn execute_tool(name: &str, input: &Value, state: &mut ReadFileState) -> ToolResult {
    match name {
        FILE_READ_TOOL_NAME => read::execute(input, state),
        FILE_WRITE_TOOL_NAME => write::execute(input, state),
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
