//! Port of GlobTool — file name patterns via walkdir + glob (parity with `rg --files`).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::ToolResult;
use crate::constants::tool_names::GLOB_TOOL_NAME;
use crate::utils::expand_path::expand_path;

const MAX_RESULTS: usize = 100;

const DESCRIPTION: &str = "- Fast file pattern matching tool that works with any codebase size
- Supports glob patterns like \"**/*.js\" or \"src/**/*.ts\"
- Returns matching file paths sorted by modification time
- Use this tool when you need to find files by name patterns
- When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead";

pub fn api_schema() -> Value {
    json!({
        "name": GLOB_TOOL_NAME,
        "description": DESCRIPTION,
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. IMPORTANT: Omit this field to use the default directory. DO NOT enter \"undefined\" or \"null\" - simply omit it for the default behavior. Must be a valid directory path if provided."
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

    let search_dir = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => match expand_path(p, None) {
            Ok(path) => {
                if !path.is_dir() {
                    return ToolResult {
                        content: format!("Path is not a directory: {}", path.display()),
                        is_error: true,
                    };
                }
                path
            }
            Err(e) => {
                return ToolResult {
                    content: e,
                    is_error: true,
                };
            }
        },
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    match glob_files(pattern, &search_dir) {
        Ok((files, truncated)) => {
            if files.is_empty() {
                return ToolResult {
                    content: "No files found".into(),
                    is_error: false,
                };
            }
            let mut out = files.join("\n");
            if truncated {
                out.push_str(
                    "\n(Results are truncated. Consider using a more specific path or pattern.)",
                );
            }
            ToolResult {
                content: out,
                is_error: false,
            }
        }
        Err(e) => ToolResult {
            content: e,
            is_error: true,
        },
    }
}

fn glob_files(pattern: &str, search_dir: &Path) -> Result<(Vec<String>, bool), String> {
    let (base, relative_pattern) = if Path::new(pattern).is_absolute() {
        extract_glob_base(pattern)
    } else {
        (search_dir.to_path_buf(), pattern.to_string())
    };

    let matcher =
        glob::Pattern::new(&relative_pattern).map_err(|e| format!("Invalid glob pattern: {e}"))?;

    let mut matches: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in walkdir::WalkDir::new(&base)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(&base).unwrap_or(path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        // Also try matching against full path relative patterns like **/foo
        if matcher.matches(&rel_str)
            || matcher.matches(path.file_name().and_then(|n| n.to_str()).unwrap_or(""))
        {
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            matches.push((mtime, path.to_path_buf()));
        }
    }

    // Sort by modified ascending (oldest first), matching rg --sort=modified
    matches.sort_by_key(|a| a.0);

    let truncated = matches.len() > MAX_RESULTS;
    let files: Vec<String> = matches
        .into_iter()
        .take(MAX_RESULTS)
        .map(|(_, p)| {
            // Prefer path relative to cwd when under cwd
            if let Ok(cwd) = std::env::current_dir()
                && let Ok(rel) = p.strip_prefix(&cwd)
            {
                return rel.display().to_string();
            }
            p.display().to_string()
        })
        .collect();

    Ok((files, truncated))
}

fn extract_glob_base(pattern: &str) -> (PathBuf, String) {
    let chars = ['*', '?', '[', '{'];
    let idx = pattern.find(|c| chars.contains(&c));
    match idx {
        None => {
            let p = Path::new(pattern);
            (
                p.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(".")),
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(pattern)
                    .to_string(),
            )
        }
        Some(i) => {
            let static_prefix = &pattern[..i];
            let last_sep = static_prefix
                .rfind('/')
                .or_else(|| static_prefix.rfind('\\'));
            match last_sep {
                Some(0) => (PathBuf::from("/"), pattern[1..].to_string()),
                Some(sep) => (
                    PathBuf::from(&pattern[..sep]),
                    pattern[sep + 1..].to_string(),
                ),
                None => (PathBuf::from("."), pattern.to_string()),
            }
        }
    }
}
