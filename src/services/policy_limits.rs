//! Port of `src/services/policyLimits/index.ts` (stub).

use crate::PortResult;

pub async fn wait_for_policy_limits_to_load() -> PortResult<()> {
    Ok(())
}

pub fn is_policy_allowed(_key: &str) -> bool {
    // Stub: allow until policy limits are ported
    true
}
