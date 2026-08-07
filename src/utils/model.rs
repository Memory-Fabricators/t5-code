//! Port of `src/utils/model/model.ts` (default main-loop model).

/// Default Sonnet model for Pro / standard subscribers.
pub const DEFAULT_SONNET_MODEL: &str = "claude-sonnet-4-6";

pub fn get_default_sonnet_model() -> String {
    DEFAULT_SONNET_MODEL.into()
}

/// Main-loop model setting (full Max/Opus selection deferred).
pub fn get_main_loop_model() -> String {
    get_default_sonnet_model()
}

pub fn resolve_model(cli_override: Option<&str>) -> String {
    cli_override
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(get_main_loop_model)
}
