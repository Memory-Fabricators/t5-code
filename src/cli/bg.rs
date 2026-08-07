//! Port of `src/cli/bg.ts` (stub).

use crate::PortResult;

pub async fn ps_handler(_args: &[String]) -> PortResult<()> {
    Err(crate::not_yet_ported("cli/bg::ps"))
}

pub async fn logs_handler(_session: Option<&str>) -> PortResult<()> {
    Err(crate::not_yet_ported("cli/bg::logs"))
}

pub async fn attach_handler(_session: Option<&str>) -> PortResult<()> {
    Err(crate::not_yet_ported("cli/bg::attach"))
}

pub async fn kill_handler(_session: Option<&str>) -> PortResult<()> {
    Err(crate::not_yet_ported("cli/bg::kill"))
}

pub async fn handle_bg_flag(_args: &[String]) -> PortResult<()> {
    Err(crate::not_yet_ported("cli/bg::handleBgFlag"))
}
