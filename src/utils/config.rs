//! Port of `src/utils/config.ts` (stub).

use crate::PortResult;

pub fn enable_configs() {
    // no-op until config is ported
}

pub fn enable_configs_result() -> PortResult<()> {
    enable_configs();
    Ok(())
}
