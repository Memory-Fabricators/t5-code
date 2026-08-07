//! Terminal event loop for the interactive REPL.

use std::io::{self, stdout};
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::crossterm::{cursor, terminal::Clear, terminal::ClearType};
use ratatui::{Terminal, TerminalOptions, Viewport};
use tokio::task::JoinHandle;

use crate::PortResult;
use crate::services::api::messages::Conversation;
use crate::utils::early_input::{consume_early_input, stop_capturing_early_input};

use super::app::{App, Role};
use super::draw;

struct TurnOutcome {
    conversation: Conversation,
    result: Result<String, String>,
}

enum Pending {
    Idle,
    Running(JoinHandle<TurnOutcome>),
}

/// Ensures raw mode / alt-screen are undone even if the REPL errors out.
struct TerminalGuard {
    alt_screen: bool,
}

impl TerminalGuard {
    fn enter(alt_screen: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        if alt_screen {
            execute!(out, EnterAlternateScreen, cursor::Hide)?;
        } else {
            // Main-buffer fullscreen (closer to default Ink): clear once, hide cursor.
            execute!(
                out,
                cursor::Hide,
                Clear(ClearType::All),
                cursor::MoveTo(0, 0)
            )?;
        }
        Ok(Self { alt_screen })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = execute!(out, cursor::Show);
        if self.alt_screen {
            let _ = execute!(out, LeaveAlternateScreen);
        } else {
            // Leave a clean line below the UI so the shell prompt isn't overwritten.
            let _ = execute!(out, Clear(ClearType::FromCursorDown));
        }
        let _ = disable_raw_mode();
    }
}

/// Enter the interactive REPL. Restores the terminal on exit.
pub async fn run(model: String) -> PortResult<()> {
    // Stop the early-input stdin thread BEFORE enabling raw mode — otherwise it
    // races crossterm for keystrokes and corrupts the display.
    stop_capturing_early_input();
    let early = consume_early_input();

    // Build conversation while still in cooked mode (network / system prompt).
    let conversation = Conversation::new(&model, 8192, false).await?;

    // Default Ink is not alternate-screen; only use it when explicitly requested.
    let alt_screen = std::env::var("CLAUDE_CODE_NO_FLICKER")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
        || std::env::var("T5_CODE_ALT_SCREEN")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

    let _guard = TerminalGuard::enter(alt_screen)
        .map_err(|e| rootcause::report!(format!("failed to initialize terminal: {e}")))?;

    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fullscreen,
        },
    )
    .map_err(|e| rootcause::report!(format!("failed to create terminal: {e}")))?;

    // Avoid ratatui's default panic hook fighting our Drop guard; install a
    // complementary one that restores via disable_raw_mode if we unwind.
    let alt = alt_screen;
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = stdout();
        let _ = execute!(out, cursor::Show);
        if alt {
            let _ = execute!(out, LeaveAlternateScreen);
        }
        let _ = disable_raw_mode();
        prev(info);
    }));

    let result = run_app(&mut terminal, model, conversation, early).await;

    // Force a final clear of the draw region so leftover cells don't linger.
    let _ = terminal.clear();

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model: String,
    conversation: Conversation,
    early: String,
) -> PortResult<()> {
    let mut app = App::new(model.clone());
    if !early.is_empty() {
        app.input = early;
    }
    let mut conversation = Some(conversation);
    let mut pending = Pending::Idle;

    loop {
        // Blink caret ~2Hz while idle.
        if !app.busy {
            app.cursor_visible = (app.spinner_started.elapsed().as_millis() / 500) % 2 == 0;
        }

        terminal
            .draw(|f| draw::draw(f, &app))
            .map_err(|e| rootcause::report!(format!("draw failed: {e}")))?;

        if app.should_quit {
            break;
        }

        // Poll in-flight query without blocking the UI.
        if matches!(&pending, Pending::Running(h) if h.is_finished()) {
            if let Pending::Running(handle) = std::mem::replace(&mut pending, Pending::Idle) {
                app.busy = false;
                match handle.await {
                    Ok(outcome) => {
                        conversation = Some(outcome.conversation);
                        match outcome.result {
                            Ok(reply) => {
                                if reply.trim().is_empty() {
                                    app.push(Role::Assistant, "(no text response)");
                                } else {
                                    app.push(Role::Assistant, reply);
                                }
                            }
                            Err(e) => {
                                app.push(Role::Error, e);
                            }
                        }
                    }
                    Err(e) => {
                        app.push(Role::Error, format!("task join error: {e}"));
                        conversation = Some(Conversation::new(&model, 8192, false).await?);
                    }
                }
            }
        }

        if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
            continue;
        }

        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        // Ignore key release events (Windows / some terminals).
        if key.kind == KeyEventKind::Release {
            continue;
        }

        match key.code {
            KeyCode::Esc => {
                app.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !app.busy {
                    app.history.clear();
                    let _ = terminal.clear();
                }
            }
            KeyCode::Char('?') if !app.busy && app.input.is_empty() => {
                app.push(
                    Role::System,
                    "Shortcuts: Enter send · Esc/Ctrl+C quit · Ctrl+L clear · PageUp/Down scroll",
                );
            }
            KeyCode::PageUp => {
                app.history_offset = app.history_offset.saturating_add(5);
            }
            KeyCode::PageDown => {
                app.history_offset = app.history_offset.saturating_sub(5);
            }
            KeyCode::Enter => {
                if app.busy {
                    continue;
                }
                let prompt = app.input.trim().to_string();
                if prompt.is_empty() {
                    continue;
                }
                if matches!(prompt.as_str(), "/exit" | "/quit") {
                    app.should_quit = true;
                    continue;
                }

                let Some(mut conv) = conversation.take() else {
                    app.push(Role::Error, "conversation unavailable");
                    continue;
                };

                app.input.clear();
                app.push(Role::User, prompt.clone());
                app.start_busy();

                let handle = tokio::spawn(async move {
                    match conv.submit(&prompt).await {
                        Ok(text) => TurnOutcome {
                            conversation: conv,
                            result: Ok(text),
                        },
                        Err(e) => TurnOutcome {
                            conversation: conv,
                            result: Err(e.to_string()),
                        },
                    }
                });
                pending = Pending::Running(handle);
            }
            KeyCode::Backspace => {
                if !app.busy {
                    app.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if !app.busy && !key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.input.push(c);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
