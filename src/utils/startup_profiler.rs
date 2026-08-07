//! Port of `src/utils/startupProfiler.ts`.
//!
//! Minimal timing checkpoints. Full Statsig/analytics reporting deferred.

use std::sync::Mutex;
use std::time::Instant;

use std::sync::OnceLock;

struct ProfilerState {
    enabled: bool,
    detailed: bool,
    start: Instant,
    checkpoints: Vec<(String, Instant)>,
}

use crate::utils::env_utils::is_env_var_truthy;

fn state() -> &'static Mutex<ProfilerState> {
    static STATE: OnceLock<Mutex<ProfilerState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let detailed = is_env_var_truthy("CLAUDE_CODE_PROFILE_STARTUP");
        // Sampled Statsig path deferred; enable when detailed or ant user type.
        let ant = std::env::var("USER_TYPE").ok().as_deref() == Some("ant");
        let enabled = detailed || ant;
        let start = Instant::now();
        let mut checkpoints = Vec::new();
        if enabled {
            checkpoints.push(("profiler_initialized".to_string(), start));
        }
        Mutex::new(ProfilerState {
            enabled,
            detailed,
            start,
            checkpoints,
        })
    })
}

/// Record a checkpoint with the given name.
pub fn profile_checkpoint(name: &str) {
    let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
    if !guard.enabled {
        return;
    }
    let now = Instant::now();
    if guard.detailed {
        let ms = now.duration_since(guard.start).as_secs_f64() * 1000.0;
        eprintln!("[startup] {name}: {ms:.2}ms");
    }
    guard.checkpoints.push((name.to_string(), now));
}
