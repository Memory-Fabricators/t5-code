//! Dark-theme tokens mirroring `src/utils/theme.ts` (default dark).

use ratatui::style::Color;

/// Claude brand coral/orange — spinner + accents.
pub const CLAUDE: Color = Color::Rgb(215, 119, 87);
/// Lighter shimmer companion (verb highlight).
pub const CLAUDE_SHIMMER: Color = Color::Rgb(235, 159, 127);
/// Prompt top/bottom hairlines.
pub const PROMPT_BORDER: Color = Color::Rgb(136, 136, 136);
/// Dim secondary text (user ❯, footer, system).
pub const SUBTLE: Color = Color::Rgb(80, 80, 80);
/// Body text.
pub const TEXT: Color = Color::Rgb(230, 230, 230);
/// Soft background tint for user messages (approximate; used when styling chips).
#[allow(dead_code)]
pub const USER_BG: Color = Color::Rgb(55, 55, 55);
/// Error text.
pub const ERROR: Color = Color::Rgb(220, 80, 80);

/// Prompt pointer (Ink `figures.pointer`).
pub const PROMPT_GLYPH: &str = "❯";
/// Assistant / tool marker (Ink `BLACK_CIRCLE` / `⏺` on macOS — use ● for broad terminals).
pub const ASSISTANT_GLYPH: &str = "●";
/// Nested tool-result indent prefix (Ink `MessageResponse`).
#[allow(dead_code)]
pub const RESULT_GLYPH: &str = "⎿";

/// Spinner glyph cycle from `Spinner/utils.ts`.
pub const SPINNER_FRAMES: &[&str] = &["·", "✢", "✳", "✶", "✻", "✽"];

/// Loading verbs from `constants/spinnerVerbs.ts` (subset + Clauding).
pub const SPINNER_VERBS: &[&str] = &[
    "Accomplishing",
    "Baking",
    "Brewing",
    "Calculating",
    "Clauding",
    "Cogitating",
    "Composing",
    "Considering",
    "Contemplating",
    "Cooking",
    "Crafting",
    "Crunching",
    "Deliberating",
    "Enchanting",
    "Envisioning",
    "Finagling",
    "Generating",
    "Hatching",
    "Ideating",
    "Marinating",
    "Mulling",
    "Muscling",
    "Noodling",
    "Percolating",
    "Pondering",
    "Processing",
    "Reticulating",
    "Scheming",
    "Sprouting",
    "Thinking",
    "Tinkering",
    "Working",
];
