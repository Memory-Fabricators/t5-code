//! Shared read-before-write validation (Write / Edit).

use std::fs;
use std::path::Path;

use super::{ReadFileEntry, ReadFileState};

pub fn file_mtime_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}

pub fn validate_read_before_write(path: &Path, state: &ReadFileState) -> Result<(), String> {
    let entry = match state.get(path) {
        Some(e) if !e.is_partial_view() => e,
        Some(_) | None => {
            return Err("File has not been read yet. Read it first before writing to it.".into());
        }
    };

    if let Some(mtime) = file_mtime_ms(path) {
        if mtime > entry.timestamp {
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

pub fn update_read_state(state: &mut ReadFileState, path: std::path::PathBuf, content: String) {
    let timestamp = file_mtime_ms(&path).unwrap_or(0);
    state.insert(
        path,
        ReadFileEntry {
            content,
            timestamp,
            offset: None,
            limit: None,
        },
    );
}
