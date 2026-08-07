//! Port of `src/bridge/bridgeEnabled.ts` (stub).

use crate::PortResult;

pub async fn get_bridge_disabled_reason() -> PortResult<Option<String>> {
    Ok(Some("not yet ported: bridge/bridgeEnabled".to_string()))
}

pub fn check_bridge_min_version() -> Option<String> {
    None
}
