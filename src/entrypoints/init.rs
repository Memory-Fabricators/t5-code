//! Port of `src/entrypoints/init.ts` (slim).
//!
//! Full telemetry / proxy / mTLS / analytics deferred; config gate + first-start
//! recording match the critical early path.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::PortResult;
use crate::utils::config::{enable_configs, record_first_start_time};
use crate::utils::startup_profiler::profile_checkpoint;

static INIT_DONE: AtomicBool = AtomicBool::new(false);

/// Memoized init — safe to call multiple times.
pub async fn init() -> PortResult<()> {
    if INIT_DONE.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    profile_checkpoint("init_function_start");

    enable_configs()?;
    profile_checkpoint("init_configs_enabled");

    // applySafeConfigEnvironmentVariables / CA certs / graceful shutdown /
    // telemetry / mTLS / proxy — deferred.
    profile_checkpoint("init_safe_env_vars_applied");

    record_first_start_time();
    profile_checkpoint("init_function_end");

    Ok(())
}
