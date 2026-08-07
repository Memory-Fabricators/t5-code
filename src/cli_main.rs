//! Port of `src/main.tsx` — clap shell skeleton (Commander parity deferred incrementally).
//!
//! Full interactive/headless agent loop is not yet ported.

use clap::{Parser, Subcommand};

/// Claude Code / t5-code CLI (Rust port in progress).
#[derive(Debug, Parser)]
#[command(
    name = "t5-code",
    version,
    about = "T5 Code (Claude Code) — Rust port",
    disable_version_flag = true
)]
pub struct Cli {
    /// Prompt to run (positional, optional)
    pub prompt: Option<String>,

    /// Print mode (non-interactive)
    #[arg(short = 'p', long = "print")]
    pub print: bool,

    /// Bare / simple mode
    #[arg(long = "bare")]
    pub bare: bool,

    /// Model override
    #[arg(long = "model")]
    pub model: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Update t5-code
    Update,
    /// Authentication commands
    Auth {
        #[command(subcommand)]
        action: Option<AuthCommands>,
    },
    /// Log in
    Login,
    /// Log out
    Logout,
    /// MCP server management
    Mcp,
    /// Doctor / diagnostics
    Doctor,
    /// Install
    Install,
}

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    Login,
    Logout,
    Status,
}

/// Entry called from `entrypoints/cli.rs` after fast-path dispatch.
pub async fn main() {
    main_with_args(std::env::args()).await;
}

/// Like [`main`], but with an explicit argv (used for `--update` → `update` rewrite).
pub async fn main_with_args<I, T>(args: I)
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Update => {
                eprintln!("not yet ported: update");
                std::process::exit(1);
            }
            Commands::Auth { action } => {
                eprintln!("not yet ported: auth {action:?}");
                std::process::exit(1);
            }
            Commands::Login => {
                eprintln!("not yet ported: login");
                std::process::exit(1);
            }
            Commands::Logout => {
                eprintln!("not yet ported: logout");
                std::process::exit(1);
            }
            Commands::Mcp => {
                eprintln!("not yet ported: mcp");
                std::process::exit(1);
            }
            Commands::Doctor => {
                eprintln!("not yet ported: doctor");
                std::process::exit(1);
            }
            Commands::Install => {
                eprintln!("not yet ported: install");
                std::process::exit(1);
            }
        }
    }

    if cli.print || cli.prompt.is_some() {
        eprintln!("not yet ported: main print/prompt path");
        std::process::exit(1);
    }

    eprintln!("not yet ported: interactive REPL (ratatui later)");
    std::process::exit(1);
}
