//! Port of `src/utils/envUtils.ts`.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Home for Claude config files: `$CLAUDE_CONFIG_DIR` or `~/.claude`.
pub fn get_claude_config_home_dir() -> PathBuf {
    static CACHED: OnceLock<PathBuf> = OnceLock::new();
    // Recompute when CLAUDE_CONFIG_DIR changes is rare; cache first observed value.
    CACHED
        .get_or_init(|| {
            if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
                && !dir.is_empty()
            {
                return PathBuf::from(dir);
            }
            home_dir().join(".claude")
        })
        .clone()
}

pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME")
        && !h.is_empty()
    {
        return PathBuf::from(h);
    }
    if let Ok(h) = std::env::var("USERPROFILE")
        && !h.is_empty()
    {
        return PathBuf::from(h);
    }
    PathBuf::from(".")
}

pub fn is_env_truthy(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) => {
            let normalized = v.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
    }
}

pub fn is_env_var_truthy(key: &str) -> bool {
    is_env_truthy(std::env::var(key).ok().as_deref())
}

/// `--bare` / `CLAUDE_CODE_SIMPLE`.
pub fn is_bare_mode() -> bool {
    is_env_var_truthy("CLAUDE_CODE_SIMPLE") || std::env::args().any(|a| a == "--bare")
}
