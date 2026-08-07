//! Port of FileWriteTool — create/overwrite with read-before-write for existing files.

use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use super::{ReadFileEntry, ReadFileState, ToolResult};
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
    if exists {
        if let Err(msg) = validate_read_before_write(&path, state) {
            return ToolResult {
                content: msg,
                is_error: true,
            };
        }
    }

    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ToolResult {
                content: format!("Failed to create directory {}: {e}", parent.display()),
                is_error: true,
            };
        }
    }

    if let Err(e) = fs::write(&path, content.as_bytes()) {
        return ToolResult {
            content: format!("Failed to write {}: {e}", path.display()),
            is_error: true,
        };
    }

    let mtime_ms = fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    state.insert(
        path.clone(),
        ReadFileEntry {
            content: content.to_string(),
            timestamp: mtime_ms,
            offset: None,
            limit: None,
        },
    );

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

fn validate_read_before_write(path: &Path, state: &ReadFileState) -> Result<(), String> {
    let entry = match state.get(path) {
        Some(e) if !e.is_partial_view() => e,
        Some(_) => {
            return Err("File has not been read yet. Read it first before writing to it.".into());
        }
        None => {
            return Err("File has not been read yet. Read it first before writing to it.".into());
        }
    };

    let mtime_ms = fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_millis() as u64).saturating_sub(0));

    if let Some(mtime) = mtime_ms {
        let last = mtime; // floor already via integer ms
        if last > entry.timestamp {
            // Content fallback for false-positive mtime bumps
            if let Ok(on_disk) = fs::read_to_string(path) {
                let normalized = on_disk.replace("\r\n", "\n");
                if normalized != entry.content {
                    return Err(
                        "File has been modified since read, either by the user or by a linter. Read it again before attempting to write it."
                            .into(),
                    );
                }
            } else {
                return Err(
                    "File has been modified since read, either by the user or by a linter. Read it again before attempting to write it."
                        .into(),
                );
            }
        }
    }

    Ok(())
}
