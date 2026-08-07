//! Port of `src/main.tsx` — clap shell skeleton (Commander parity deferred incrementally).
//!
//! Interactive REPL: ratatui TUI. `-p`: non-interactive tool loop.

use clap::{Parser, Subcommand};

use crate::cli::handlers::auth::{auth_login, auth_logout, auth_status};
use crate::entrypoints::init;
use crate::utils::config::{
    get_global_config, global_config_file_exists, global_config_path_string,
};
use crate::utils::startup_profiler::profile_checkpoint;

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
        action: AuthCommands,
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
    /// Log in (browser OAuth deferred; supports refresh-token env)
    Login,
    /// Log out
    Logout,
    /// Show auth status (JSON by default)
    Status {
        /// Emit human-readable text instead of JSON
        #[arg(long)]
        text: bool,
    },
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

    profile_checkpoint("preAction_start");
    if let Err(e) = init::init().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
    crate::utils::sinks::init_sinks();
    profile_checkpoint("preAction_after_init");

    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::Update => {
                eprintln!("not yet ported: update");
                std::process::exit(1);
            }
            Commands::Auth { action } => {
                let result = match action {
                    AuthCommands::Login => auth_login().await,
                    AuthCommands::Logout => auth_logout().await,
                    AuthCommands::Status { text } => auth_status(!text).await,
                };
                if let Err(e) = result {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return;
            }
            Commands::Login => {
                if let Err(e) = auth_login().await {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return;
            }
            Commands::Logout => {
                if let Err(e) = auth_logout().await {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
                return;
            }
            Commands::Mcp => {
                eprintln!("not yet ported: mcp");
                std::process::exit(1);
            }
            Commands::Doctor => {
                run_doctor();
                return;
            }
            Commands::Install => {
                eprintln!("not yet ported: install");
                std::process::exit(1);
            }
        }
    }

    if cli.print || cli.prompt.is_some() {
        let prompt = match &cli.prompt {
            Some(p) if !p.is_empty() => p.clone(),
            _ => {
                eprintln!("Error: prompt required for print mode (e.g. t5-code -p \"hello\")");
                std::process::exit(1);
            }
        };
        let model = crate::utils::model::resolve_model(cli.model.as_deref());
        match crate::services::api::messages::create_message(&model, &prompt, 8192).await {
            Ok(text) => {
                println!("{text}");
                return;
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }

    let model = crate::utils::model::resolve_model(cli.model.as_deref());
    if let Err(e) = crate::tui::run(model).await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run_doctor() {
    let path = global_config_path_string();
    let status = if global_config_file_exists() {
        "ok (file present)"
    } else {
        "ok (missing → defaults)"
    };
    let cfg = get_global_config();
    let (source, has_token) = crate::utils::auth::get_auth_token_source();
    println!("T5 Code (Claude Code) doctor");
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    println!("  config:  {path} — {status}");
    if let Some(t) = &cfg.first_start_time {
        println!("  firstStartTime: {t}");
    }
    println!("  theme: {}", cfg.theme);
    println!(
        "  auth:   {} ({})",
        if has_token {
            "logged in"
        } else {
            "not logged in"
        },
        source.as_str()
    );
    println!("ok");
}
