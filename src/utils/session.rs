//! Port of session persistence (`sessionStorage.ts` / `sessionStoragePortable.ts`).
//!
//! Append-only JSONL under `~/.claude/projects/<sanitized-cwd>/<sessionId>.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::utils::env_utils::get_claude_config_home_dir;

const MAX_SANITIZED_LENGTH: usize = 200;
const UUID_RE: &str = r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$";

static SESSION_ID: Mutex<Option<String>> = Mutex::new(None);
static LAST_PARENT_UUID: Mutex<Option<String>> = Mutex::new(None);
static PERSISTED_COUNT: Mutex<usize> = Mutex::new(0);

/// Generate a UUID v4 string.
pub fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    rand::fill(&mut bytes[..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

pub fn validate_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    let _ = UUID_RE; // documentation
    true
}

/// Replace non-alphanumeric with `-`; truncate + hash if over 200 chars.
pub fn sanitize_path(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if sanitized.len() <= MAX_SANITIZED_LENGTH {
        return sanitized;
    }
    let hash = simple_hash(name);
    format!("{}-{hash}", &sanitized[..MAX_SANITIZED_LENGTH])
}

fn simple_hash(s: &str) -> String {
    let mut h: u32 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(u32::from(b));
    }
    format!("{h:x}")
}

pub fn get_projects_dir() -> PathBuf {
    get_claude_config_home_dir().join("projects")
}

pub fn get_project_dir(cwd: &str) -> PathBuf {
    get_projects_dir().join(sanitize_path(cwd))
}

pub fn current_cwd_string() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".into())
}

pub fn get_session_id() -> String {
    let mut guard = SESSION_ID.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(new_uuid());
    }
    guard.clone().unwrap()
}

pub fn set_session_id(id: String) {
    *SESSION_ID.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
    *LAST_PARENT_UUID.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *PERSISTED_COUNT.lock().unwrap_or_else(|e| e.into_inner()) = 0;
}

/// Mint a new session id (e.g. `/clear`). Old JSONL file is left on disk.
pub fn regenerate_session_id() -> String {
    let id = new_uuid();
    set_session_id(id.clone());
    id
}

pub fn get_transcript_path() -> PathBuf {
    get_transcript_path_for_session(&get_session_id())
}

pub fn get_transcript_path_for_session(session_id: &str) -> PathBuf {
    get_project_dir(&current_cwd_string()).join(format!("{session_id}.jsonl"))
}

fn iso_now() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Approximate RFC3339 without chrono dependency.
    format!("{ms}")
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}

fn append_jsonl(path: &Path, entry: &Value) -> Result<(), String> {
    ensure_parent(path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    Ok(())
}

/// Persist any new API messages since the last flush (parentUuid chain).
pub fn persist_new_messages(messages: &Value) -> Result<(), String> {
    let Some(arr) = messages.as_array() else {
        return Ok(());
    };
    let mut count = PERSISTED_COUNT.lock().unwrap_or_else(|e| e.into_inner());
    if *count >= arr.len() {
        return Ok(());
    }

    let session_id = get_session_id();
    let path = get_transcript_path_for_session(&session_id);
    let cwd = current_cwd_string();
    let mut parent = LAST_PARENT_UUID.lock().unwrap_or_else(|e| e.into_inner());

    for msg in &arr[*count..] {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        // Skip pure system-reminder injections that aren't real turns? Keep all.
        let uuid = new_uuid();
        let entry = json!({
            "parentUuid": parent.clone(),
            "isSidechain": false,
            "type": role,
            "message": msg,
            "uuid": uuid,
            "timestamp": iso_now(),
            "cwd": cwd,
            "sessionId": session_id,
            "version": env!("CARGO_PKG_VERSION"),
            "userType": "external",
        });
        append_jsonl(&path, &entry)?;
        *parent = Some(uuid);
    }
    *count = arr.len();

    // Update last-prompt metadata for /resume listing.
    if let Some(text) = first_user_text(arr) {
        let flat = text.replace('\n', " ");
        let flat = if flat.len() > 200 {
            format!("{}…", flat.chars().take(200).collect::<String>().trim())
        } else {
            flat
        };
        let meta = json!({
            "type": "last-prompt",
            "lastPrompt": flat,
            "sessionId": session_id,
        });
        append_jsonl(&path, &meta)?;
    }

    Ok(())
}

fn first_user_text(arr: &[Value]) -> Option<String> {
    for msg in arr.iter().rev() {
        if msg.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        match msg.get("content") {
            Some(Value::String(s)) if !s.is_empty() && !s.contains("<system-reminder>") => {
                return Some(s.clone());
            }
            Some(Value::Array(parts)) => {
                let text: String = parts
                    .iter()
                    .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

/// Reset persist cursor after `/clear` (new empty session).
pub fn reset_persist_cursor() {
    *LAST_PARENT_UUID.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *PERSISTED_COUNT.lock().unwrap_or_else(|e| e.into_inner()) = 0;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub path: PathBuf,
    pub mtime_ms: u64,
    pub last_prompt: Option<String>,
    pub custom_title: Option<String>,
    pub message_count: usize,
}

/// List sessions for the current project dir (newest first).
pub fn list_sessions(limit: usize) -> Vec<SessionSummary> {
    let dir = get_project_dir(&current_cwd_string());
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if !validate_uuid(&stem) {
            continue;
        }
        let meta = entry.metadata().ok();
        let mtime_ms = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let (last_prompt, custom_title, message_count) = read_lite_meta(&path);
        sessions.push(SessionSummary {
            session_id: stem,
            path,
            mtime_ms,
            last_prompt,
            custom_title,
            message_count,
        });
    }
    sessions.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    sessions.truncate(limit);
    sessions
}

fn read_lite_meta(path: &Path) -> (Option<String>, Option<String>, usize) {
    let Ok(file) = fs::File::open(path) else {
        return (None, None, 0);
    };
    let reader = BufReader::new(file);
    let mut last_prompt = None;
    let mut custom_title = None;
    let mut message_count = 0;
    for line in reader.lines().map_while(Result::ok) {
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "user" | "assistant" => {
                if !v
                    .get("isSidechain")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false)
                {
                    message_count += 1;
                }
            }
            "last-prompt" => {
                last_prompt = v
                    .get("lastPrompt")
                    .and_then(|t| t.as_str())
                    .map(str::to_string);
            }
            "custom-title" => {
                custom_title = v
                    .get("customTitle")
                    .and_then(|t| t.as_str())
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    (last_prompt, custom_title, message_count)
}

/// Load API messages from a transcript by walking the parentUuid chain.
pub fn load_messages_from_transcript(session_id: &str) -> Result<Vec<Value>, String> {
    let path = get_transcript_path_for_session(session_id);
    if !path.exists() {
        return Err(format!("Session not found: {session_id}"));
    }
    let file = fs::File::open(&path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);

    let mut by_uuid: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if ty != "user" && ty != "assistant" {
            continue;
        }
        if v.get("isSidechain")
            .and_then(|b| b.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(uuid) = v.get("uuid").and_then(|u| u.as_str()).map(str::to_string) else {
            continue;
        };
        order.push(uuid.clone());
        by_uuid.insert(uuid, v);
    }

    if order.is_empty() {
        return Ok(Vec::new());
    }

    // Walk from newest leaf via parentUuid.
    let mut chain_ids = Vec::new();
    let mut cur = order.last().cloned();
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = cur {
        if !seen.insert(id.clone()) {
            break;
        }
        chain_ids.push(id.clone());
        cur = by_uuid
            .get(&id)
            .and_then(|e| e.get("parentUuid"))
            .and_then(|p| {
                if p.is_null() {
                    None
                } else {
                    p.as_str().map(str::to_string)
                }
            });
    }
    chain_ids.reverse();

    let mut messages = Vec::new();
    let mut last_uuid = None;
    for id in chain_ids {
        if let Some(entry) = by_uuid.get(&id)
            && let Some(msg) = entry.get("message")
        {
            messages.push(msg.clone());
            last_uuid = Some(id);
        }
    }

    // Adopt session for continued appends.
    set_session_id(session_id.to_string());
    *LAST_PARENT_UUID.lock().unwrap_or_else(|e| e.into_inner()) = last_uuid;
    *PERSISTED_COUNT.lock().unwrap_or_else(|e| e.into_inner()) = messages.len();

    Ok(messages)
}

/// Most recently modified session in this project, if any.
pub fn most_recent_session_id() -> Option<String> {
    list_sessions(1).into_iter().next().map(|s| s.session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_slashes() {
        let s = sanitize_path("/Users/theo/proj");
        assert!(s.starts_with("-Users-theo-proj") || s.contains("Users"));
        assert!(!s.contains('/'));
    }

    #[test]
    fn uuid_roundtrip_shape() {
        let id = new_uuid();
        assert!(validate_uuid(&id), "{id}");
    }
}
