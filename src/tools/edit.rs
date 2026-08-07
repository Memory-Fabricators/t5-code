//! Port of FileEditTool — exact string replace with read-before-write.

use serde_json::{Value, json};
use std::fs;

use super::state::{file_mtime_ms, update_read_state, validate_read_before_write};
use super::{ReadFileState, ToolResult};
use crate::constants::tool_names::{FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME};
use crate::utils::expand_path::expand_path;

const DESCRIPTION: &str = "Performs exact string replacements in files.";

fn prompt() -> String {
    format!(
        "Performs exact string replacements in files.\n\
\n\
Usage:\n\
- You must use your `{FILE_READ_TOOL_NAME}` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file.\n\
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: line number + tab. Everything after that is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.\n\
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.\n\
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.\n\
- The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use `replace_all` to change every instance of `old_string`.\n\
- Use `replace_all` for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance."
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": FILE_EDIT_TOOL_NAME,
        "description": format!("{DESCRIPTION}\n\n{}", prompt()),
        "input_schema": {
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "default": false,
                    "description": "Replace all occurrences of old_string (default false)"
                }
            },
            "required": ["file_path", "old_string", "new_string"],
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
    let old_string = match input.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                content: "Missing required parameter: old_string".into(),
                is_error: true,
            };
        }
    };
    let new_string = match input.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                content: "Missing required parameter: new_string".into(),
                is_error: true,
            };
        }
    };
    let replace_all = input
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if old_string == new_string {
        return ToolResult {
            content: "No changes to make: old_string and new_string are exactly the same.".into(),
            is_error: true,
        };
    }

    let path = match expand_path(file_path, None) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                content: e,
                is_error: true,
            };
        }
    };

    if path.extension().and_then(|e| e.to_str()) == Some("ipynb") {
        return ToolResult {
            content: "File is a Jupyter Notebook. Notebook editing is not yet ported.".into(),
            is_error: true,
        };
    }

    let exists = path.exists();

    // Empty old_string → create / fill empty file
    if old_string.is_empty() {
        if exists {
            let on_disk = fs::read_to_string(&path).unwrap_or_default();
            if !on_disk.trim().is_empty() {
                return ToolResult {
                    content: "Cannot create new file - file already exists.".into(),
                    is_error: true,
                };
            }
        } else if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult {
                    content: format!("Failed to create directory {}: {e}", parent.display()),
                    is_error: true,
                };
            }
        }
        if let Err(e) = fs::write(&path, new_string.as_bytes()) {
            return ToolResult {
                content: format!("Failed to write {}: {e}", path.display()),
                is_error: true,
            };
        }
        update_read_state(state, path, new_string.to_string());
        return ToolResult {
            content: format!("The file {file_path} has been updated successfully."),
            is_error: false,
        };
    }

    if !exists {
        return ToolResult {
            content: format!(
                "File does not exist. Current working directory: {}.",
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".into())
            ),
            is_error: true,
        };
    }

    if let Err(msg) = validate_read_before_write(&path, state) {
        return ToolResult {
            content: msg,
            is_error: true,
        };
    }

    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return ToolResult {
                content: format!("Failed to read {}: {e}", path.display()),
                is_error: true,
            };
        }
    };
    let file = raw.replace("\r\n", "\n");

    let actual_old = match find_actual_string(&file, old_string) {
        Some(s) => s,
        None => {
            return ToolResult {
                content: format!("String to replace not found in file.\nString: {old_string}"),
                is_error: true,
            };
        }
    };

    let matches = file.matches(actual_old.as_str()).count();
    if matches == 0 {
        return ToolResult {
            content: format!("String to replace not found in file.\nString: {old_string}"),
            is_error: true,
        };
    }
    if matches > 1 && !replace_all {
        return ToolResult {
            content: format!(
                "Found {matches} matches of the string to replace, but replace_all is false. \
To replace all occurrences, set replace_all to true. To replace only one occurrence, please provide more context to uniquely identify the instance.\n\
String: {old_string}"
            ),
            is_error: true,
        };
    }

    let new_content = if replace_all {
        file.replace(actual_old.as_str(), new_string)
    } else {
        file.replacen(actual_old.as_str(), new_string, 1)
    };

    // Preserve original EOL if file was CRLF
    let to_write = if raw.contains("\r\n") {
        new_content.replace('\n', "\r\n")
    } else {
        new_content.clone()
    };

    if let Err(e) = fs::write(&path, to_write.as_bytes()) {
        return ToolResult {
            content: format!("Failed to write {}: {e}", path.display()),
            is_error: true,
        };
    }

    let _ = file_mtime_ms(&path);
    update_read_state(state, path, new_content);

    let msg = if replace_all {
        format!(
            "The file {file_path} has been updated. All occurrences were successfully replaced."
        )
    } else {
        format!("The file {file_path} has been updated successfully.")
    };

    ToolResult {
        content: msg,
        is_error: false,
    }
}

fn find_actual_string(file_content: &str, search: &str) -> Option<String> {
    if file_content.contains(search) {
        return Some(search.to_string());
    }
    let normalized_search = normalize_quotes(search);
    let normalized_file = normalize_quotes(file_content);
    if let Some(idx) = normalized_file.find(&normalized_search) {
        return Some(file_content[idx..idx + search.len()].to_string());
    }
    None
}

fn normalize_quotes(s: &str) -> String {
    s.replace(['\u{201C}', '\u{201D}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'")
}
