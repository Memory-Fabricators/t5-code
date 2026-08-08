//! Slim plan file helpers (port of `src/utils/plans.ts`).

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::utils::env_utils::get_claude_config_home_dir;

static PLAN_SLUG: Mutex<Option<String>> = Mutex::new(None);

pub fn get_plans_directory() -> PathBuf {
    let dir = get_claude_config_home_dir().join("plans");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn word_slug() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("plan-{:x}", n % 0xffff_ffff)
}

pub fn get_plan_slug() -> String {
    let mut guard = PLAN_SLUG.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(word_slug());
    }
    guard.clone().unwrap()
}

pub fn clear_plan_slug() {
    *PLAN_SLUG.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn get_plan_file_path() -> PathBuf {
    get_plans_directory().join(format!("{}.md", get_plan_slug()))
}

pub fn get_plan() -> Option<String> {
    let path = get_plan_file_path();
    fs::read_to_string(path).ok()
}

pub fn ensure_plan_file() -> PathBuf {
    let path = get_plan_file_path();
    if !path.exists() {
        let _ = fs::write(
            &path,
            "# Plan\n\nWrite your plan here. Use ExitPlanMode when ready for approval.\n",
        );
    }
    path
}
