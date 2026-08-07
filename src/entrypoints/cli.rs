//! Port of `src/entrypoints/cli.tsx`.
//! Bootstrap entrypoint — checks special flags before loading the full CLI.

use t5_code::feature;
use t5_code::utils::startup_profiler::profile_checkpoint;

fn apply_env_tweaks() {
    // Bugfix for corepack auto-pinning
    // SAFETY: single-threaded at process start before any threads spawn.
    unsafe {
        std::env::set_var("COREPACK_ENABLE_AUTO_PIN", "0");
    }

    if std::env::var("CLAUDE_CODE_REMOTE").ok().as_deref() == Some("true") {
        let existing = std::env::var("NODE_OPTIONS").unwrap_or_default();
        let new_val = if existing.is_empty() {
            "--max-old-space-size=8192".to_string()
        } else {
            format!("{existing} --max-old-space-size=8192")
        };
        unsafe {
            std::env::set_var("NODE_OPTIONS", new_val);
        }
    }

    if feature("ABLATION_BASELINE") && std::env::var_os("CLAUDE_CODE_ABLATION_BASELINE").is_some() {
        for k in [
            "CLAUDE_CODE_SIMPLE",
            "CLAUDE_CODE_DISABLE_THINKING",
            "DISABLE_INTERLEAVED_THINKING",
            "DISABLE_COMPACT",
            "DISABLE_AUTO_COMPACT",
            "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
            "CLAUDE_CODE_DISABLE_BACKGROUND_TASKS",
        ] {
            if std::env::var_os(k).is_none() {
                unsafe {
                    std::env::set_var(k, "1");
                }
            }
        }
    }
}

fn exit_err(e: impl std::fmt::Display) -> ! {
    eprintln!("{e}");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    apply_env_tweaks();

    let args: Vec<String> = std::env::args().skip(1).collect();

    // Fast-path for --version/-v: zero module loading needed
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-v" | "-V") {
        println!("T5 Code (Claude Code)");
        return;
    }

    profile_checkpoint("cli_entry");

    // Fast-path for --dump-system-prompt
    if feature("DUMP_SYSTEM_PROMPT")
        && args.first().map(String::as_str) == Some("--dump-system-prompt")
    {
        profile_checkpoint("cli_dump_system_prompt_path");
        t5_code::utils::config::enable_configs();
        let model = {
            let model_idx = args.iter().position(|a| a == "--model");
            match model_idx {
                Some(i) => args
                    .get(i + 1)
                    .cloned()
                    .unwrap_or_else(t5_code::utils::model::get_main_loop_model),
                None => t5_code::utils::model::get_main_loop_model(),
            }
        };
        match t5_code::constants::prompts::get_system_prompt(&[], &model).await {
            Ok(prompt) => println!("{}", prompt.join("\n")),
            Err(e) => exit_err(e),
        }
        return;
    }

    if args.first().map(String::as_str) == Some("--claude-in-chrome-mcp") {
        profile_checkpoint("cli_claude_in_chrome_mcp_path");
        if let Err(e) =
            t5_code::utils::claude_in_chrome::mcp_server::run_claude_in_chrome_mcp_server().await
        {
            exit_err(e);
        }
        return;
    } else if args.first().map(String::as_str) == Some("--chrome-native-host") {
        profile_checkpoint("cli_chrome_native_host_path");
        if let Err(e) =
            t5_code::utils::claude_in_chrome::chrome_native_host::run_chrome_native_host().await
        {
            exit_err(e);
        }
        return;
    } else if feature("CHICAGO_MCP")
        && args.first().map(String::as_str) == Some("--computer-use-mcp")
    {
        profile_checkpoint("cli_computer_use_mcp_path");
        if let Err(e) =
            t5_code::utils::computer_use::mcp_server::run_computer_use_mcp_server().await
        {
            exit_err(e);
        }
        return;
    }

    // Fast-path for `--daemon-worker` (internal)
    if feature("DAEMON") && args.first().map(String::as_str) == Some("--daemon-worker") {
        let kind = args.get(1).map(String::as_str);
        if let Err(e) = t5_code::daemon::worker_registry::run_daemon_worker(kind).await {
            exit_err(e);
        }
        return;
    }

    // Fast-path for remote-control / bridge aliases
    if feature("BRIDGE_MODE")
        && matches!(
            args.first().map(String::as_str),
            Some("remote-control" | "rc" | "remote" | "sync" | "bridge")
        )
    {
        profile_checkpoint("cli_bridge_path");
        t5_code::utils::config::enable_configs();

        if t5_code::utils::auth::get_claude_ai_oauth_tokens()
            .and_then(|t| t.access_token)
            .is_none()
        {
            t5_code::utils::process::exit_with_error(t5_code::bridge::types::BRIDGE_LOGIN_ERROR);
        }

        match t5_code::bridge::bridge_enabled::get_bridge_disabled_reason().await {
            Ok(Some(reason)) => {
                t5_code::utils::process::exit_with_error(&format!("Error: {reason}"));
            }
            Ok(None) => {}
            Err(e) => t5_code::utils::process::exit_with_error(&e.to_string()),
        }

        if let Some(version_error) = t5_code::bridge::bridge_enabled::check_bridge_min_version() {
            t5_code::utils::process::exit_with_error(&version_error);
        }

        if let Err(e) = t5_code::services::policy_limits::wait_for_policy_limits_to_load().await {
            t5_code::utils::process::exit_with_error(&e.to_string());
        }
        if !t5_code::services::policy_limits::is_policy_allowed("allow_remote_control") {
            t5_code::utils::process::exit_with_error(
                "Error: Remote Control is disabled by your organization's policy.",
            );
        }

        let rest: Vec<String> = args.iter().skip(1).cloned().collect();
        if let Err(e) = t5_code::bridge::bridge_main::bridge_main(&rest).await {
            exit_err(e);
        }
        return;
    }

    // Fast-path for `daemon`
    if feature("DAEMON") && args.first().map(String::as_str) == Some("daemon") {
        profile_checkpoint("cli_daemon_path");
        t5_code::utils::config::enable_configs();
        t5_code::utils::sinks::init_sinks();
        let rest: Vec<String> = args.iter().skip(1).cloned().collect();
        if let Err(e) = t5_code::daemon::main::daemon_main(&rest).await {
            exit_err(e);
        }
        return;
    }

    // Fast-path for ps|logs|attach|kill and --bg/--background
    if feature("BG_SESSIONS")
        && (matches!(
            args.first().map(String::as_str),
            Some("ps" | "logs" | "attach" | "kill")
        ) || args.iter().any(|a| a == "--bg" || a == "--background"))
    {
        profile_checkpoint("cli_bg_path");
        t5_code::utils::config::enable_configs();
        let result = match args.first().map(String::as_str) {
            Some("ps") => {
                let rest: Vec<String> = args.iter().skip(1).cloned().collect();
                t5_code::cli::bg::ps_handler(&rest).await
            }
            Some("logs") => t5_code::cli::bg::logs_handler(args.get(1).map(String::as_str)).await,
            Some("attach") => {
                t5_code::cli::bg::attach_handler(args.get(1).map(String::as_str)).await
            }
            Some("kill") => t5_code::cli::bg::kill_handler(args.get(1).map(String::as_str)).await,
            _ => t5_code::cli::bg::handle_bg_flag(&args).await,
        };
        if let Err(e) = result {
            exit_err(e);
        }
        return;
    }

    // Fast-path for template job commands
    if feature("TEMPLATES")
        && matches!(
            args.first().map(String::as_str),
            Some("new" | "list" | "reply")
        )
    {
        profile_checkpoint("cli_templates_path");
        if let Err(e) = t5_code::cli::handlers::template_jobs::templates_main(&args).await {
            exit_err(e);
        }
        std::process::exit(0);
    }

    // Fast-path for environment-runner
    if feature("BYOC_ENVIRONMENT_RUNNER")
        && args.first().map(String::as_str) == Some("environment-runner")
    {
        profile_checkpoint("cli_environment_runner_path");
        let rest: Vec<String> = args.iter().skip(1).cloned().collect();
        if let Err(e) = t5_code::environment_runner::environment_runner_main(&rest).await {
            exit_err(e);
        }
        return;
    }

    // Fast-path for self-hosted-runner
    if feature("SELF_HOSTED_RUNNER")
        && args.first().map(String::as_str) == Some("self-hosted-runner")
    {
        profile_checkpoint("cli_self_hosted_runner_path");
        let rest: Vec<String> = args.iter().skip(1).cloned().collect();
        if let Err(e) = t5_code::self_hosted_runner::self_hosted_runner_main(&rest).await {
            exit_err(e);
        }
        return;
    }

    // Fast-path for --worktree --tmux
    let has_tmux_flag = args.iter().any(|a| a == "--tmux" || a == "--tmux=classic");
    if has_tmux_flag
        && (args.iter().any(|a| a == "-w" || a == "--worktree")
            || args.iter().any(|a| a.starts_with("--worktree=")))
    {
        profile_checkpoint("cli_tmux_worktree_fast_path");
        t5_code::utils::config::enable_configs();
        if t5_code::utils::worktree_mode_enabled::is_worktree_mode_enabled() {
            match t5_code::utils::worktree::exec_into_tmux_worktree(&args).await {
                Ok(result) if result.handled => return,
                Ok(result) => {
                    if let Some(error) = result.error {
                        t5_code::utils::process::exit_with_error(&error);
                    }
                }
                Err(e) => t5_code::utils::process::exit_with_error(&e.to_string()),
            }
        }
    }

    // Redirect common update flag mistakes to the update subcommand
    let mut argv: Vec<String> = std::env::args().collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--update" | "--upgrade") {
        if argv.len() >= 2 {
            argv[1] = "update".to_string();
        } else {
            argv.push("update".to_string());
        }
    }

    // --bare: set SIMPLE early so gates fire during option building
    if args.iter().any(|a| a == "--bare") {
        unsafe {
            std::env::set_var("CLAUDE_CODE_SIMPLE", "1");
        }
    }

    // No special flags detected, load and run the full CLI
    t5_code::utils::early_input::start_capturing_early_input();
    profile_checkpoint("cli_before_main_import");
    profile_checkpoint("cli_after_main_import");
    t5_code::cli_main::main_with_args(argv).await;
    profile_checkpoint("cli_after_main_complete");
}
