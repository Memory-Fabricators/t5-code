//! Port of `src/utils/env.ts` (path helpers only for now).

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::constants::oauth::file_suffix_for_oauth_config;
use crate::utils::env_utils::{get_claude_config_home_dir, home_dir};

/// Global Claude config file path (`~/.claude.json` or legacy `~/.claude/.config.json`).
pub fn get_global_claude_file() -> PathBuf {
    static CACHED: OnceLock<PathBuf> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            let legacy = get_claude_config_home_dir().join(".config.json");
            if legacy.is_file() {
                return legacy;
            }
            let filename = format!(".claude{}.json", file_suffix_for_oauth_config());
            // Prefer CLAUDE_CONFIG_DIR parent layout matching TS:
            // join(process.env.CLAUDE_CONFIG_DIR || homedir(), filename)
            if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
                && !dir.is_empty()
            {
                return PathBuf::from(dir).join(filename);
            }
            home_dir().join(filename)
        })
        .clone()
}
