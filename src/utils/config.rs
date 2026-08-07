//! Port of `src/utils/config.ts` — enableConfigs / global config read-write spine.
//!
//! Full GlobalConfig surface and lock/freshness watcher are deferred; unknown
//! JSON fields are preserved via `extra` so round-trips do not wipe auth.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PortResult;
use crate::utils::env::get_global_claude_file;
use crate::utils::errors::ConfigParseError;

static CONFIG_READING_ALLOWED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    #[serde(default)]
    pub num_startups: u64,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_notif_channel")]
    pub preferred_notif_channel: String,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default = "default_editor_mode")]
    pub editor_mode: String,
    #[serde(default = "default_true")]
    pub auto_compact_enabled: bool,
    #[serde(default = "default_true")]
    pub show_turn_duration: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub tips_history: HashMap<String, u64>,
    #[serde(default)]
    pub memory_usage_count: u64,
    #[serde(default)]
    pub first_start_time: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub oauth_account: Option<crate::utils::auth::OauthAccountInfo>,
    /// Preserves unported / unknown fields across read-modify-write.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

fn default_theme() -> String {
    "dark".into()
}
fn default_notif_channel() -> String {
    "auto".into()
}
fn default_editor_mode() -> String {
    "normal".into()
}
fn default_true() -> bool {
    true
}

impl Default for GlobalConfig {
    fn default() -> Self {
        create_default_global_config()
    }
}

pub fn create_default_global_config() -> GlobalConfig {
    GlobalConfig {
        num_startups: 0,
        theme: default_theme(),
        preferred_notif_channel: default_notif_channel(),
        verbose: false,
        editor_mode: default_editor_mode(),
        auto_compact_enabled: true,
        show_turn_duration: true,
        env: HashMap::new(),
        tips_history: HashMap::new(),
        memory_usage_count: 0,
        first_start_time: None,
        user_id: None,
        oauth_account: None,
        extra: serde_json::Map::new(),
    }
}

struct GlobalConfigCache {
    config: Option<GlobalConfig>,
}

fn cache() -> &'static Mutex<GlobalConfigCache> {
    static CACHE: std::sync::OnceLock<Mutex<GlobalConfigCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(GlobalConfigCache { config: None }))
}

/// Idempotent gate: allow config reads and validate the global config file.
/// Panics are avoided — corrupt config returns [`ConfigParseError`].
pub fn enable_configs() -> Result<(), ConfigParseError> {
    if CONFIG_READING_ALLOWED.load(Ordering::SeqCst) {
        return Ok(());
    }
    CONFIG_READING_ALLOWED.store(true, Ordering::SeqCst);
    get_config(&get_global_claude_file(), true)?;
    Ok(())
}

/// Backward-compatible wrapper used by the CLI bootstrap (ignores parse errors
/// only when called from places that previously treated this as void). Prefer
/// [`enable_configs`] and handle the error.
pub fn enable_configs_infallible() {
    if let Err(e) = enable_configs() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

pub fn enable_configs_result() -> PortResult<()> {
    Ok(enable_configs()?)
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{FEFF}').unwrap_or(s)
}

fn get_config(file: &Path, throw_on_invalid: bool) -> Result<GlobalConfig, ConfigParseError> {
    if !CONFIG_READING_ALLOWED.load(Ordering::SeqCst)
        && std::env::var("NODE_ENV").ok().as_deref() != Some("test")
    {
        return Err(ConfigParseError::new(
            "Config accessed before allowed.",
            file.display().to_string(),
        ));
    }

    match fs::read_to_string(file) {
        Ok(content) => match serde_json::from_str::<GlobalConfig>(strip_bom(&content)) {
            Ok(mut cfg) => {
                if cfg.theme.is_empty() {
                    cfg.theme = default_theme();
                }
                Ok(cfg)
            }
            Err(e) => {
                let err = ConfigParseError::new(e.to_string(), file.display().to_string());
                if throw_on_invalid {
                    Err(err)
                } else {
                    eprintln!(
                        "\nClaude configuration file at {} is corrupted: {}\n",
                        file.display(),
                        err.message
                    );
                    Ok(create_default_global_config())
                }
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(create_default_global_config()),
        Err(e) => Err(ConfigParseError::new(
            e.to_string(),
            file.display().to_string(),
        )),
    }
}

pub fn get_global_config() -> GlobalConfig {
    {
        let guard = cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cfg) = guard.config.clone() {
            return cfg;
        }
    }

    let file = get_global_claude_file();
    let config = get_config(&file, false).unwrap_or_else(|_| create_default_global_config());
    if let Ok(mut guard) = cache().lock() {
        guard.config = Some(config.clone());
    }
    config
}

pub fn save_global_config(updater: impl FnOnce(GlobalConfig) -> GlobalConfig) {
    let file = get_global_claude_file();
    let current = get_config(&file, false).unwrap_or_else(|_| create_default_global_config());
    let updated = updater(current);
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&updated) {
        Ok(json) => {
            if let Err(e) = fs::write(&file, json) {
                eprintln!("Failed to save config: {e}");
                return;
            }
            if let Ok(mut guard) = cache().lock() {
                guard.config = Some(updated);
            }
        }
        Err(e) => eprintln!("Failed to serialize config: {e}"),
    }
}

pub fn record_first_start_time() {
    let config = get_global_config();
    if config.first_start_time.is_none() {
        let first_start_time = unix_timestamp_secs_string();
        save_global_config(|mut current| {
            if current.first_start_time.is_none() {
                current.first_start_time = Some(first_start_time.clone());
            }
            current
        });
    }
}

fn unix_timestamp_secs_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub fn global_config_path_string() -> String {
    get_global_claude_file().display().to_string()
}

pub fn global_config_file_exists() -> bool {
    get_global_claude_file().is_file()
}
