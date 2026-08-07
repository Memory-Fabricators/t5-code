//! Port of FileWriteTool — create/overwrite with read-before-write for existing files.

use serde_json::{Value, json};
use std::fs;

use super::state::{update_read_state, validate_read_before_write};
use super::{ReadFileState, ToolResult};
use crate::constants::tool_names::{FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME};
use crate::utils::expand_path::expand_path;

const DESCRIPTION: &str = "Write a file to the local filesystem.";

fn prompt() -> String {
    format!(
        "Writes a file to the local filesystem.\n\
\n\
Usage:\n\
- This tool will overwrite the existing file if there is one at the provided path.\n\
- If this is an existing file, you MUST use the {FILE_READ_TOOL_NAME} tool first to read the file's contents. This tool will fail if you did not read the file first.\n\
- Prefer the Edit tool for modifying existing files — it only sends the diff. Only use this tool to create new files or for complete rewrites.\n\
- NEVER create documentation files (*.md) or README files unless explicitly requested by the User.\n\
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked."
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": FILE_WRITE_TOOL_NAME,
        "description": format!("{DESCRIPTION}\n\n{}", prompt()),
        "input_schema": {
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"],
            "additionalProperties": false
        }
    })
}

pub fn execute(input: &Value, state: &mut ReadFileState) -> ToolResult {
    let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                content: "Missing required parameter: file_path".into(),
                is_error: true,
            };
        }
    };
    let content = match input.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                content: "Missing required parameter: content".into(),
                is_error: true,
            };
        }
    };

    let path = match expand_path(file_path, None) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                content: e,
                is_error: true,
            };
        }
    };

    let exists = path.exists();
    if exists && let Err(msg) = validate_read_before_write(&path, state) {
        return ToolResult {
            content: msg,
            is_error: true,
        };
    }

    if let Some(parent) = path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        return ToolResult {
            content: format!("Failed to create directory {}: {e}", parent.display()),
            is_error: true,
        };
    }

    if let Err(e) = fs::write(&path, content.as_bytes()) {
        return ToolResult {
            content: format!("Failed to write {}: {e}", path.display()),
            is_error: true,
        };
    }

    update_read_state(state, path, content.to_string());

    let msg = if exists {
        format!("The file {file_path} has been updated successfully.")
    } else {
        format!("File created successfully at: {file_path}")
    };

    ToolResult {
        content: msg,
        is_error: false,
    }
}
