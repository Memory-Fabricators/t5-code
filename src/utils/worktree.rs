//! Port of `src/utils/worktree.ts` (stub).

use crate::PortResult;

#[derive(Debug, Default)]
pub struct TmuxWorktreeResult {
    pub handled: bool,
    pub error: Option<String>,
}

pub async fn exec_into_tmux_worktree(_args: &[String]) -> PortResult<TmuxWorktreeResult> {
    Ok(TmuxWorktreeResult {
        handled: false,
        error: Some("not yet ported: utils/worktree".to_string()),
    })
}
