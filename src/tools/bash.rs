//! Port of BashTool — embedded `brush-core` shell (Windows-friendly).

use serde_json::{Value, json};
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use super::ToolResult;
use crate::constants::tool_names::{
    BASH_TOOL_NAME, FILE_EDIT_TOOL_NAME, FILE_READ_TOOL_NAME, FILE_WRITE_TOOL_NAME, GLOB_TOOL_NAME,
    GREP_TOOL_NAME,
};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_TIMEOUT_MS: u64 = 600_000;

fn description() -> String {
    format!(
        "Executes a given bash command and returns its output.\n\
\n\
The working directory persists between commands, but shell state (env vars, etc.) does not.\n\
\n\
IMPORTANT: Avoid using this tool to run find, grep, cat, head, tail, sed, awk, or echo commands \
that only read or write files, unless explicitly instructed. Prefer dedicated tools:\n\
- File search: {GLOB_TOOL_NAME} / {GREP_TOOL_NAME}\n\
- Read files: {FILE_READ_TOOL_NAME}\n\
- Edit files: {FILE_EDIT_TOOL_NAME} / {FILE_WRITE_TOOL_NAME}\n\
\n\
You may specify an optional timeout in milliseconds (up to {MAX_TIMEOUT_MS}ms). \
By default, commands timeout after {DEFAULT_TIMEOUT_MS}ms."
    )
}

pub fn api_schema() -> Value {
    json!({
        "name": BASH_TOOL_NAME,
        "description": description(),
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": format!("Optional timeout in milliseconds (max {MAX_TIMEOUT_MS})")
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does in 5-10 words"
                }
            },
            "required": ["command"],
            "additionalProperties": false
        }
    })
}

/// Run a bash command via brush; `cwd` is updated when the command changes directory.
pub async fn execute(input: &Value, cwd: &mut PathBuf) -> ToolResult {
    let command = match input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                content: "Missing required parameter: command".into(),
                is_error: true,
            };
        }
    };

    let timeout_ms = input
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .min(MAX_TIMEOUT_MS);

    let run = run_brush(command, cwd);
    let result = match tokio::time::timeout(Duration::from_millis(timeout_ms), run).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return ToolResult {
                content: format!("Bash error: {e}"),
                is_error: true,
            };
        }
        Err(_) => {
            return ToolResult {
                content: format!(
                    "<error>Command exceeded timeout of {timeout_ms}ms and was aborted</error>"
                ),
                is_error: true,
            };
        }
    };

    let mut parts = Vec::new();
    let stdout = result
        .stdout
        .trim_start_matches(|c: char| c == '\n' || c == '\r');
    let stdout = stdout.trim_end();
    if !stdout.is_empty() {
        parts.push(stdout.to_string());
    }
    let stderr = result.stderr.trim();
    if !stderr.is_empty() {
        parts.push(stderr.to_string());
    }
    if result.exit_code != 0 {
        parts.push(format!("Exit code {}", result.exit_code));
    }

    ToolResult {
        content: if parts.is_empty() {
            String::new()
        } else {
            parts.join("\n")
        },
        is_error: result.exit_code != 0,
    }
}

struct BashOutput {
    stdout: String,
    stderr: String,
    exit_code: u8,
}

async fn run_brush(command: &str, cwd: &mut PathBuf) -> Result<BashOutput, String> {
    use brush_core::openfiles::{OpenFile, OpenFiles};
    use brush_core::{ProfileLoadBehavior, RcLoadBehavior};

    let mut shell = brush_core::Shell::builder()
        .working_dir(cwd.clone())
        .profile(ProfileLoadBehavior::Skip)
        .rc(RcLoadBehavior::Skip)
        .build()
        .await
        .map_err(|e| e.to_string())?;

    let (mut stdout_r, stdout_w) = std::io::pipe().map_err(|e| e.to_string())?;
    let (mut stderr_r, stderr_w) = std::io::pipe().map_err(|e| e.to_string())?;

    let mut params = shell.default_exec_params();
    params.set_fd(OpenFiles::STDOUT_FD, OpenFile::from(stdout_w));
    params.set_fd(OpenFiles::STDERR_FD, OpenFile::from(stderr_w));

    let exec = shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await
        .map_err(|e| e.to_string())?;

    drop(params);

    let mut stdout = String::new();
    stdout_r
        .read_to_string(&mut stdout)
        .map_err(|e| e.to_string())?;
    let mut stderr = String::new();
    stderr_r
        .read_to_string(&mut stderr)
        .map_err(|e| e.to_string())?;

    // Persist cwd across Bash calls (shell env does not persist).
    *cwd = shell.working_dir().to_path_buf();

    Ok(BashOutput {
        stdout,
        stderr,
        exit_code: u8::from(exec.exit_code),
    })
}
