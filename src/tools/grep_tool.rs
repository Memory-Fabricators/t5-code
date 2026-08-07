//! Port of GrepTool — embedded `uu_grep` (GNU grep in Rust) via stdout capture.

use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

use super::ToolResult;
use crate::constants::tool_names::{BASH_TOOL_NAME, GREP_TOOL_NAME};
use crate::utils::expand_path::expand_path;

/// Serialize stdout capture — `uu_grep` writes to process stdout.
static GREP_STDOUT_LOCK: Mutex<()> = Mutex::new(());

const DEFAULT_HEAD_LIMIT: usize = 250;

fn description() -> String {
    format!(
        "A powerful search tool built on an embedded GNU-compatible grep (uutils)\n\
\n\
  Usage:\n\
  - ALWAYS use {GREP_TOOL_NAME} for search tasks. NEVER invoke `grep` or `rg` as a {BASH_TOOL_NAME} command.\n\
  - Supports full regex syntax (e.g., \"log.*Error\", \"function\\\\s+\\\\w+\")\n\
  - Filter files with glob parameter (e.g., \"*.js\", \"**/*.tsx\") or type parameter (e.g., \"js\", \"py\", \"rust\")\n\
  - Output modes: \"content\" shows matching lines, \"files_with_matches\" shows only file paths (default), \"count\" shows match counts\n\
  - Pattern syntax: Uses grep (not ripgrep) — literal braces may need escaping\n\
  - Multiline matching: By default patterns match within single lines only. For cross-line patterns, use `multiline: true`\n"
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": GREP_TOOL_NAME,
        "description": description(),
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regular expression pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. \"*.js\", \"*.{ts,tsx}\") — maps to grep --include"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode. Defaults to \"files_with_matches\"."
                },
                "-B": {
                    "type": "integer",
                    "description": "Number of lines to show before each match. Requires output_mode: \"content\"."
                },
                "-A": {
                    "type": "integer",
                    "description": "Number of lines to show after each match. Requires output_mode: \"content\"."
                },
                "-C": {
                    "type": "integer",
                    "description": "Alias for context."
                },
                "context": {
                    "type": "integer",
                    "description": "Number of lines to show before and after each match. Requires output_mode: \"content\"."
                },
                "-n": {
                    "type": "boolean",
                    "description": "Show line numbers. Requires output_mode: \"content\". Defaults to true."
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "type": {
                    "type": "string",
                    "description": "File type filter (approximate via --include=*.EXT)"
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Limit output to first N lines/entries. Defaults to 250. Pass 0 for unlimited."
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip first N lines/entries before applying head_limit. Defaults to 0."
                },
                "multiline": {
                    "type": "boolean",
                    "description": "Enable multiline matching (uses Perl regex with (?s)). Default: false."
                }
            },
            "required": ["pattern"],
            "additionalProperties": false
        }
    })
}

pub fn execute(input: &Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                content: "Missing required parameter: pattern".into(),
                is_error: true,
            };
        }
    };

    let search_path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => match expand_path(p, None) {
            Ok(path) => path,
            Err(e) => {
                return ToolResult {
                    content: e,
                    is_error: true,
                };
            }
        },
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    if !search_path.exists() {
        return ToolResult {
            content: format!("Path does not exist: {}", search_path.display()),
            is_error: true,
        };
    }

    let output_mode = input
        .get("output_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("files_with_matches");
    let case_insensitive = input.get("-i").and_then(|v| v.as_bool()).unwrap_or(false);
    let show_line_numbers = input.get("-n").and_then(|v| v.as_bool()).unwrap_or(true);
    let multiline = input
        .get("multiline")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let head_limit = input
        .get("head_limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let offset = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let mut args: Vec<std::ffi::OsString> = vec!["grep".into(), "--color=never".into()];

    // Recurse into directories (GNU grep); for a single file this is fine.
    if search_path.is_dir() {
        args.push("-r".into());
        for dir in [".git", ".svn", ".hg", ".bzr", "node_modules"] {
            args.push(format!("--exclude-dir={dir}").into());
        }
    }

    if case_insensitive {
        args.push("-i".into());
    }

    match output_mode {
        "files_with_matches" => args.push("-l".into()),
        "count" => args.push("-c".into()),
        "content" => {
            if show_line_numbers {
                args.push("-n".into());
            }
            let ctx = input
                .get("context")
                .or_else(|| input.get("-C"))
                .and_then(|v| v.as_u64());
            if let Some(c) = ctx {
                args.push("-C".into());
                args.push(c.to_string().into());
            } else {
                if let Some(b) = input.get("-B").and_then(|v| v.as_u64()) {
                    args.push("-B".into());
                    args.push(b.to_string().into());
                }
                if let Some(a) = input.get("-A").and_then(|v| v.as_u64()) {
                    args.push("-A".into());
                    args.push(a.to_string().into());
                }
            }
        }
        other => {
            return ToolResult {
                content: format!("Invalid output_mode: {other}"),
                is_error: true,
            };
        }
    }

    if let Some(g) = input.get("glob").and_then(|v| v.as_str()) {
        // Split brace expansions naively: *.{ts,tsx} → two --include
        for part in expand_brace_glob(g) {
            args.push(format!("--include={part}").into());
        }
    }

    if let Some(ty) = input.get("type").and_then(|v| v.as_str()) {
        let ext = match ty {
            "rust" => "rs",
            "python" | "py" => "py",
            "javascript" | "js" => "js",
            "typescript" | "ts" => "ts",
            "go" => "go",
            "java" => "java",
            other => other,
        };
        args.push(format!("--include=*.{ext}").into());
    }

    if multiline {
        args.push("-P".into());
        args.push("-e".into());
        args.push(format!("(?s){pattern}").into());
    } else {
        args.push("-e".into());
        args.push(pattern.into());
    }

    args.push(search_path.as_os_str().into());

    let (exit, raw_out) = match run_uu_grep(args) {
        Ok(v) => v,
        Err(e) => {
            return ToolResult {
                content: e,
                is_error: true,
            };
        }
    };

    // GNU grep: 0 = match, 1 = no match, >=2 = error
    if exit >= 2 {
        return ToolResult {
            content: if raw_out.is_empty() {
                format!("grep failed with exit code {exit}")
            } else {
                raw_out
            },
            is_error: true,
        };
    }

    let mut lines: Vec<&str> = if raw_out.is_empty() {
        Vec::new()
    } else {
        raw_out.lines().collect()
    };

    // Relativize absolute paths under cwd for cleaner output
    let cwd = std::env::current_dir().ok();
    let relativized: Vec<String> = lines
        .drain(..)
        .map(|line| relativize_line(line, cwd.as_deref()))
        .collect();

    let applied_offset = offset.min(relativized.len());
    let sliced = &relativized[applied_offset..];
    let limit = match head_limit {
        Some(0) => sliced.len(),
        Some(n) => n,
        None => DEFAULT_HEAD_LIMIT,
    };
    let truncated = sliced.len() > limit;
    let final_lines: Vec<&str> = sliced.iter().take(limit).map(|s| s.as_str()).collect();

    let content = match output_mode {
        "files_with_matches" => {
            if final_lines.is_empty() {
                "No files found".into()
            } else {
                let n = final_lines.len();
                let plural = if n == 1 { "file" } else { "files" };
                let mut s = format!("Found {n} {plural}\n{}", final_lines.join("\n"));
                if truncated {
                    s.push_str("\n(Results are truncated. Use offset/head_limit to page.)");
                }
                s
            }
        }
        "count" => {
            if final_lines.is_empty() {
                "No matches found".into()
            } else {
                let mut s = final_lines.join("\n");
                if truncated {
                    s.push_str("\n(Results are truncated. Use offset/head_limit to page.)");
                }
                s
            }
        }
        _ => {
            if final_lines.is_empty() {
                "No matches found".into()
            } else {
                let mut s = final_lines.join("\n");
                if truncated {
                    s.push_str("\n(Results are truncated. Use offset/head_limit to page.)");
                }
                s
            }
        }
    };

    ToolResult {
        content,
        is_error: false,
    }
}

fn run_uu_grep(args: Vec<std::ffi::OsString>) -> Result<(i32, String), String> {
    let _guard = GREP_STDOUT_LOCK
        .lock()
        .map_err(|_| "grep stdout lock poisoned".to_string())?;
    let mut buf = gag::BufferRedirect::stdout().map_err(|e| e.to_string())?;
    let exit = uu_grep::uumain(args.into_iter());
    let mut output = String::new();
    buf.read_to_string(&mut output).map_err(|e| e.to_string())?;
    Ok((exit, output))
}

fn relativize_line(line: &str, cwd: Option<&std::path::Path>) -> String {
    let Some(cwd) = cwd else {
        return line.to_string();
    };
    let cwd_str = cwd.to_string_lossy();
    let prefixes = [format!("{cwd_str}/"), format!("{cwd_str}")];
    for prefix in &prefixes {
        if let Some(rest) = line.strip_prefix(prefix.as_str()) {
            // content mode: path:line:text or path-line-text
            return rest.to_string();
        }
    }
    line.to_string()
}

/// Minimal `{a,b}` expansion for --include patterns.
fn expand_brace_glob(pattern: &str) -> Vec<String> {
    if let (Some(start), Some(end)) = (pattern.find('{'), pattern.find('}'))
        && start < end
    {
        let prefix = &pattern[..start];
        let suffix = &pattern[end + 1..];
        let inner = &pattern[start + 1..end];
        return inner
            .split(',')
            .map(|part| format!("{prefix}{part}{suffix}"))
            .collect();
    }
    vec![pattern.to_string()]
}
