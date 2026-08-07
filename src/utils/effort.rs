//! Port of `src/utils/effort.ts` — effort levels for model picker + API.

use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Max => "Max",
        }
    }

    /// Glyphs from `constants/figures.ts`.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Low => "○",
            Self::Medium => "◐",
            Self::High => "●",
            Self::Max => "◉",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "max" => Some(Self::Max),
            _ => None,
        }
    }
}

/// Session effort from ModelPicker / future `/effort` (process-local).
static SESSION_EFFORT: Mutex<Option<EffortLevel>> = Mutex::new(None);

pub fn set_session_effort(level: Option<EffortLevel>) {
    if let Ok(mut g) = SESSION_EFFORT.lock() {
        *g = level;
    }
}

pub fn get_session_effort() -> Option<EffortLevel> {
    SESSION_EFFORT.lock().ok().and_then(|g| *g)
}

/// Whether the model accepts `output_config.effort`.
pub fn model_supports_effort(model: &str) -> bool {
    if crate::utils::env_utils::is_env_var_truthy("CLAUDE_CODE_ALWAYS_ENABLE_EFFORT") {
        return true;
    }
    let m = model.to_ascii_lowercase();
    if m.contains("opus-4-6") || m.contains("sonnet-4-6") {
        return true;
    }
    if m.contains("haiku") || m.contains("sonnet") || m.contains("opus") {
        return false;
    }
    // Unknown model strings on first-party default to supported.
    true
}

/// `'max'` is Opus 4.6 only for public models.
pub fn model_supports_max_effort(model: &str) -> bool {
    model.to_ascii_lowercase().contains("opus-4-6")
}

/// Default effort for a model (undefined → API high). Opus 4.6 → medium.
pub fn get_default_effort_for_model(model: &str) -> Option<EffortLevel> {
    if model.to_ascii_lowercase().contains("opus-4-6") {
        return Some(EffortLevel::Medium);
    }
    if model_supports_effort(model) {
        // Ultrathink / GrowthBook paths omitted — fall through to API default.
        return None;
    }
    None
}

pub fn get_default_effort_level_for_model(model: &str) -> EffortLevel {
    get_default_effort_for_model(model).unwrap_or(EffortLevel::High)
}

/// Env → session → model default. `None` means omit from the request body.
pub fn resolve_applied_effort(model: &str, session: Option<EffortLevel>) -> Option<EffortLevel> {
    if let Ok(env) = std::env::var("CLAUDE_CODE_EFFORT_LEVEL") {
        let lower = env.to_ascii_lowercase();
        if lower == "unset" || lower == "auto" {
            return None;
        }
        if let Some(level) = EffortLevel::parse(&env) {
            return Some(clamp_max(model, level));
        }
    }
    let resolved = session.or_else(|| get_default_effort_for_model(model));
    resolved.map(|l| clamp_max(model, l))
}

/// Level shown in the status bar / footer (API default is high when unset).
pub fn get_displayed_effort_level(model: &str, session: Option<EffortLevel>) -> EffortLevel {
    resolve_applied_effort(model, session).unwrap_or(EffortLevel::High)
}

/// Cycle session effort and return the new displayed level, or `None` if unsupported.
pub fn cycle_session_effort(model: &str, direction: CycleDir) -> Option<EffortLevel> {
    if !model_supports_effort(model) {
        return None;
    }
    let current = get_displayed_effort_level(model, get_session_effort());
    let include_max = model_supports_max_effort(model);
    let next = cycle_effort(current, direction, include_max);
    set_session_effort(Some(next));
    Some(next)
}

fn clamp_max(model: &str, level: EffortLevel) -> EffortLevel {
    if level == EffortLevel::Max && !model_supports_max_effort(model) {
        EffortLevel::High
    } else {
        level
    }
}

pub fn cycle_effort(current: EffortLevel, direction: CycleDir, include_max: bool) -> EffortLevel {
    let levels: &[EffortLevel] = if include_max {
        &[
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Max,
        ]
    } else {
        &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]
    };
    let idx = levels
        .iter()
        .position(|l| *l == current)
        .unwrap_or_else(|| {
            levels
                .iter()
                .position(|l| *l == EffortLevel::High)
                .unwrap_or(0)
        });
    match direction {
        CycleDir::Right => levels[(idx + 1) % levels.len()],
        CycleDir::Left => levels[(idx + levels.len() - 1) % levels.len()],
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CycleDir {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_supports_max() {
        assert!(model_supports_effort("claude-opus-4-6"));
        assert!(model_supports_max_effort("claude-opus-4-6"));
        assert!(!model_supports_max_effort("claude-sonnet-4-6"));
    }

    #[test]
    fn haiku_no_effort() {
        assert!(!model_supports_effort("claude-haiku-4-5-20251001"));
    }

    #[test]
    fn cycle_wraps() {
        assert_eq!(
            cycle_effort(EffortLevel::High, CycleDir::Right, false),
            EffortLevel::Low
        );
        assert_eq!(
            cycle_effort(EffortLevel::High, CycleDir::Right, true),
            EffortLevel::Max
        );
    }

    #[test]
    fn displayed_defaults_high() {
        set_session_effort(None);
        assert_eq!(
            get_displayed_effort_level("claude-sonnet-4-6", None),
            EffortLevel::High
        );
        assert_eq!(
            get_displayed_effort_level("claude-opus-4-6", None),
            EffortLevel::Medium
        );
    }

    #[test]
    fn cycle_session_updates() {
        set_session_effort(None);
        let next = cycle_session_effort("claude-sonnet-4-6", CycleDir::Right).unwrap();
        // high → low
        assert_eq!(next, EffortLevel::Low);
        assert_eq!(get_session_effort(), Some(EffortLevel::Low));
        set_session_effort(None);
    }
}
