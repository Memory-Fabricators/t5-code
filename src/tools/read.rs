//! Port of FileReadTool — text files only (images/PDF/notebooks deferred).

use serde_json::{Value, json};
use std::fs;
use std::path::Path;

use super::{ReadFileEntry, ReadFileState, ToolResult};
use crate::constants::tool_names::FILE_READ_TOOL_NAME;
use crate::utils::expand_path::expand_path;

/// Matches `MAX_OUTPUT_SIZE` in `utils/file.ts`.
const MAX_OUTPUT_SIZE: u64 = (0.25 * 1024.0 * 1024.0) as u64;
const MAX_LINES_TO_READ: usize = 2000;

const DESCRIPTION: &str = "Read a file from the local filesystem.";

fn prompt() -> String {
    format!(
        "Reads a file from the local filesystem. You can access any file directly by using this tool.\n\
Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.\n\
\n\
Usage:\n\
- The file_path parameter must be an absolute path, not a relative path\n\
- By default, it reads up to {MAX_LINES_TO_READ} lines starting from the beginning of the file\n\
- You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters\n\
- Results are returned using cat -n format, with line numbers starting at 1\n\
- This tool can only read files, not directories. To read a directory, use an ls command via the Bash tool.\n\
- If you read a file that exists but has empty contents you will receive a system reminder warning in place of file contents."
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": FILE_READ_TOOL_NAME,
        "description": format!("{DESCRIPTION}\n\n{}", prompt()),
        "input_schema": {
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "The line number to start reading from. Only provide if the file is too large to read at once"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The number of lines to read. Only provide if the file is too large to read at once."
                }
            },
            "required": ["file_path"],
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

    let path = match expand_path(file_path, None) {
        Ok(p) => p,
        Err(e) => {
            return ToolResult {
                content: e,
                is_error: true,
            };
        }
    };

    if is_blocked_device(&path) {
        return ToolResult {
            content: format!("Cannot read device path: {}", path.display()),
            is_error: true,
        };
    }

    let meta = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) => {
            return ToolResult {
                content: format!("Failed to read {}: {e}", path.display()),
                is_error: true,
            };
        }
    };

    if meta.is_dir() {
        return ToolResult {
            content: format!(
                "Path is a directory, not a file: {}. Use Bash ls to list directories.",
                path.display()
            ),
            is_error: true,
        };
    }

    // TS: offset === 0 ? 0 : (offset ?? 1) - 1  → 1-indexed default 1
    let offset_raw = input.get("offset").and_then(|v| v.as_u64());
    let line_offset_zero = match offset_raw {
        Some(0) => 0usize,
        Some(n) => n.saturating_sub(1) as usize,
        None => 0usize,
    };
    let display_start = if offset_raw == Some(0) {
        1usize
    } else {
        offset_raw.unwrap_or(1) as usize
    };
    let limit = input.get("limit").and_then(|v| v.as_u64());

    let file_bytes = meta.len();
    if limit.is_none() && file_bytes > MAX_OUTPUT_SIZE {
        return ToolResult {
            content: format!(
                "File content ({file_bytes} bytes) exceeds maximum allowed size ({} bytes). \
Please use offset and limit parameters to read specific portions of the file, or use the GrepTool to search for specific content.",
                MAX_OUTPUT_SIZE
            ),
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

    // Normalize CRLF for state comparison (Write path uses this).
    let normalized = raw.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    // Trailing empty from final newline: keep as TS split does
    let total_lines = if normalized.is_empty() {
        0
    } else if normalized.ends_with('\n') {
        lines.len() - 1
    } else {
        lines.len()
    };

    let start_idx = line_offset_zero;
    let end_idx = match limit {
        Some(lim) => (start_idx + lim as usize).min(total_lines),
        None => (start_idx + MAX_LINES_TO_READ).min(total_lines),
    };

    if start_idx >= total_lines && total_lines > 0 {
        return ToolResult {
            content: format!("Offset {display_start} is beyond end of file ({total_lines} lines)."),
            is_error: true,
        };
    }

    let slice = if total_lines == 0 {
        String::new()
    } else {
        lines[start_idx..end_idx].join("\n")
    };

    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let is_full_read = limit.is_none() && matches!(offset_raw, None | Some(0) | Some(1));
    let stored_offset = if is_full_read {
        None
    } else {
        Some(offset_raw.unwrap_or(1))
    };

    state.insert(
        path.clone(),
        ReadFileEntry {
            content: normalized.clone(),
            timestamp: mtime_ms,
            offset: stored_offset,
            limit,
        },
    );

    if slice.is_empty() && total_lines == 0 {
        return ToolResult {
            content: "<system-reminder>Warning: the file exists but the contents are empty.</system-reminder>".into(),
            is_error: false,
        };
    }

    let numbered = add_line_numbers(&slice, display_start);
    ToolResult {
        content: numbered,
        is_error: false,
    }
}

fn add_line_numbers(content: &str, start_line: usize) -> String {
    if content.is_empty() {
        return String::new();
    }
    content
        .split('\n')
        .enumerate()
        .map(|(i, line)| format!("{}\t{line}", i + start_line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_blocked_device(path: &Path) -> bool {
    let s = path.to_string_lossy();
    matches!(
        s.as_ref(),
        "/dev/zero" | "/dev/random" | "/dev/urandom" | "/dev/full" | "/dev/null"
    ) || s.starts_with("/proc/")
}
