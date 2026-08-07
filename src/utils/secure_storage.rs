//! Port of `src/utils/secureStorage/plainTextStorage.ts`.
//!
//! Linux / default: `~/.claude/.credentials.json` (mode 0600).
//! macOS keychain deferred — plaintext is the fallback there too.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::utils::env_utils::get_claude_config_home_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub subscription_type: Option<String>,
    #[serde(default)]
    pub rate_limit_tiers: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecureStorageData {
    #[serde(default)]
    pub claude_ai_oauth: Option<OAuthTokens>,
}

fn storage_path() -> PathBuf {
    get_claude_config_home_dir().join(".credentials.json")
}

pub fn credentials_path() -> PathBuf {
    storage_path()
}

pub fn read() -> Option<SecureStorageData> {
    let path = storage_path();
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(data.trim_start_matches('\u{FEFF}')).ok()
}

pub fn update(data: &SecureStorageData) -> Result<(), String> {
    let path = storage_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string(data).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn delete() -> bool {
    let path = storage_path();
    match fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}
