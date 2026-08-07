//! Slim port of `utils/claudemd.ts` — load CLAUDE.md hierarchy for `-p` user context.
//!
//! Skips @includes, managed policy paths, AutoMem/TeamMem, and conditional rules for now.

use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::env_utils::{get_claude_config_home_dir, is_env_var_truthy};

const MEMORY_INSTRUCTION_PROMPT: &str = "Codebase and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryType {
    User,
    Project,
    Local,
}

struct MemoryFile {
    path: PathBuf,
    kind: MemoryType,
    content: String,
}

/// Build the `claudeMd` user-context string (may be empty).
pub fn get_claude_mds() -> String {
    if is_env_var_truthy("CLAUDE_CODE_DISABLE_CLAUDE_MDS") {
        return String::new();
    }

    let files = load_memory_files();
    let mut memories = Vec::new();
    for file in files {
        let description = match file.kind {
            MemoryType::Project => " (project instructions, checked into the codebase)",
            MemoryType::Local => " (user's private project instructions, not checked in)",
            MemoryType::User => " (user's private global instructions for all projects)",
        };
        let content = file.content.trim();
        if content.is_empty() {
            continue;
        }
        memories.push(format!(
            "Contents of {}{description}:\n\n{content}",
            file.path.display()
        ));
    }

    if memories.is_empty() {
        return String::new();
    }
    format!("{MEMORY_INSTRUCTION_PROMPT}\n\n{}", memories.join("\n\n"))
}

/// Today's date as YYYY-MM-DD (local preferred, UTC fallback).
pub fn local_iso_date() -> String {
    // Prefer local date(1); fall back to UTC.
    if let Ok(o) = std::process::Command::new("date")
        .args(["+%Y-%m-%d"])
        .output()
        && o.status.success()
    {
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !s.is_empty() {
            return s;
        }
    }
    if let Ok(o) = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        && o.status.success()
    {
        return String::from_utf8_lossy(&o.stdout).trim().to_string();
    }
    "unknown".into()
}

/// User-context map entries for `prepend_user_context` (claudeMd + currentDate).
pub fn get_user_context() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let claude_md = get_claude_mds();
    if !claude_md.is_empty() {
        out.push(("claudeMd".into(), claude_md));
    }
    out.push((
        "currentDate".into(),
        format!("Today's date is {}.", local_iso_date()),
    ));
    out
}

/// Prepend a meta system-reminder user message (TS `prependUserContext`).
pub fn prepend_user_context(prompt: &str, context: &[(String, String)]) -> serde_json::Value {
    if context.is_empty() {
        return serde_json::json!([
            { "role": "user", "content": prompt }
        ]);
    }
    let body = context
        .iter()
        .map(|(k, v)| format!("# {k}\n{v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let reminder = format!(
        "<system-reminder>\nAs you answer the user's questions, you can use the following context:\n{body}\n\n      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.\n</system-reminder>\n"
    );
    serde_json::json!([
        { "role": "user", "content": reminder },
        { "role": "user", "content": prompt }
    ])
}

fn load_memory_files() -> Vec<MemoryFile> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // User ~/.claude/CLAUDE.md
    let config_home = get_claude_config_home_dir();
    push_file(
        &mut result,
        &mut seen,
        config_home.join("CLAUDE.md"),
        MemoryType::User,
    );
    push_rules_dir(
        &mut result,
        &mut seen,
        &config_home.join("rules"),
        MemoryType::User,
    );

    // Walk cwd → root: Project CLAUDE.md / .claude/CLAUDE.md / rules, then Local
    let Ok(cwd) = std::env::current_dir() else {
        return result;
    };
    let mut dirs = Vec::new();
    let mut cur = cwd.clone();
    loop {
        dirs.push(cur.clone());
        match cur.parent() {
            Some(p) if p != cur => cur = p.to_path_buf(),
            _ => break,
        }
    }
    dirs.reverse(); // root → cwd so nearer files load later (higher attention)

    for dir in dirs {
        push_file(
            &mut result,
            &mut seen,
            dir.join("CLAUDE.md"),
            MemoryType::Project,
        );
        push_file(
            &mut result,
            &mut seen,
            dir.join(".claude").join("CLAUDE.md"),
            MemoryType::Project,
        );
        push_rules_dir(
            &mut result,
            &mut seen,
            &dir.join(".claude").join("rules"),
            MemoryType::Project,
        );
        push_file(
            &mut result,
            &mut seen,
            dir.join("CLAUDE.local.md"),
            MemoryType::Local,
        );
    }

    result
}

fn push_file(
    out: &mut Vec<MemoryFile>,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: PathBuf,
    kind: MemoryType,
) {
    let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
    if !seen.insert(canon) {
        return;
    }
    match fs::read_to_string(&path) {
        Ok(content) if !content.trim().is_empty() => {
            out.push(MemoryFile {
                path,
                kind,
                content,
            });
        }
        _ => {}
    }
}

fn push_rules_dir(
    out: &mut Vec<MemoryFile>,
    seen: &mut std::collections::HashSet<PathBuf>,
    dir: &Path,
    kind: MemoryType,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    paths.sort();
    for path in paths {
        push_file(out, seen, path, kind);
    }
}
