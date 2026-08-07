//! t5-code library — incremental Rust port of the Claude Code sourcemap tree.
//! TypeScript sources remain beside these modules as the reference.

extern crate alloc;

use rootcause::Report;

pub mod bridge;
pub mod cli;
pub mod cli_main;
pub mod constants;
pub mod daemon;
pub mod entrypoints;
pub mod environment_runner;
pub mod self_hosted_runner;
pub mod services;
pub mod utils;

/// Mirror of bun:bundle `feature("X")`. Cargo features are snake_case;
/// pass the TS name (e.g. `"BRIDGE_MODE"`).
pub fn feature(name: &str) -> bool {
    matches!(
        name,
        "ABLATION_BASELINE"
            | "DUMP_SYSTEM_PROMPT"
            | "CHICAGO_MCP"
            | "DAEMON"
            | "BRIDGE_MODE"
            | "BG_SESSIONS"
            | "TEMPLATES"
            | "BYOC_ENVIRONMENT_RUNNER"
            | "SELF_HOSTED_RUNNER"
    )
}

/// Placeholder until a callee module is fully ported.
#[derive(Debug, Clone)]
pub struct NotYetPorted {
    pub module: &'static str,
}

impl std::fmt::Display for NotYetPorted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not yet ported: {}", self.module)
    }
}

impl std::error::Error for NotYetPorted {}

pub type PortResult<T> = Result<T, Report>;

/// Build a [`Report`] for an unported module path.
pub fn not_yet_ported(module: &'static str) -> Report {
    rootcause::report!(NotYetPorted { module }).into()
}
