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
use crate::commands::{self, CommandAction, parse_slash};
use crate::services::api::messages::Conversation;
use crate::utils::early_input::{consume_early_input, stop_capturing_early_input};
use crate::utils::effort::CycleDir;

use super::app::{App, Role};
use super::draw;
use super::model_picker::PickerResult;

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
        app.set_input(early);
    }
    let mut conversation = Some(conversation);
    let mut pending = Pending::Idle;

    loop {
        // Blink caret ~2Hz while idle.
        if !app.busy {
            app.cursor_visible =
                (app.spinner_started.elapsed().as_millis() / 500).is_multiple_of(2);
        }

        terminal
            .draw(|f| draw::draw(f, &app))
            .map_err(|e| rootcause::report!(format!("draw failed: {e}")))?;

        if app.should_quit {
            break;
        }

        // Poll in-flight query without blocking the UI.
        if matches!(&pending, Pending::Running(h) if h.is_finished())
            && let Pending::Running(handle) = std::mem::replace(&mut pending, Pending::Idle)
        {
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
                if let Some(picker) = app.model_picker.take() {
                    let result = picker.cancel(&app.model);
                    apply_picker_result(&mut app, &mut conversation, result).await;
                } else if !app.suggestions.is_empty() {
                    app.suggestions.clear();
                } else {
                    app.should_quit = true;
                }
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
            }
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !app.busy && app.model_picker.is_none() {
                    app.history.clear();
                    let _ = terminal.clear();
                }
            }
            KeyCode::Char('?')
                if !app.busy && app.input.is_empty() && app.model_picker.is_none() =>
            {
                app.push(
                    Role::System,
                    "Shortcuts: Enter send · Tab/Shift+Tab effort · Tab complete slash · Esc/Ctrl+C quit · /model",
                );
            }
            KeyCode::PageUp if app.model_picker.is_none() && app.suggestions.is_empty() => {
                app.history_offset = app.history_offset.saturating_add(5);
            }
            KeyCode::PageDown if app.model_picker.is_none() && app.suggestions.is_empty() => {
                app.history_offset = app.history_offset.saturating_sub(5);
            }
            KeyCode::Left if app.model_picker.is_some() => {
                if let Some(p) = app.model_picker.as_mut() {
                    p.cycle_effort_left();
                }
            }
            KeyCode::Right if app.model_picker.is_some() => {
                if let Some(p) = app.model_picker.as_mut() {
                    p.cycle_effort_right();
                }
            }
            KeyCode::Up | KeyCode::Char('k') if app.model_picker.is_some() => {
                if let Some(p) = app.model_picker.as_mut() {
                    p.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') if app.model_picker.is_some() => {
                if let Some(p) = app.model_picker.as_mut() {
                    p.move_down();
                }
            }
            KeyCode::Up if !app.suggestions.is_empty() => {
                app.suggestions.move_up();
            }
            KeyCode::Down if !app.suggestions.is_empty() => {
                app.suggestions.move_down();
            }
            // Shift+Tab → lower effort (or previous suggestion when menu open).
            KeyCode::BackTab if !app.busy && app.model_picker.is_none() => {
                if !app.suggestions.is_empty() {
                    app.suggestions.move_up();
                } else if let Some(level) = app.cycle_effort(CycleDir::Left)
                    && let Some(conv) = conversation.as_mut()
                {
                    conv.set_effort(Some(level));
                }
            }
            // Tab → accept slash suggestion, else raise effort.
            KeyCode::Tab if !app.busy && app.model_picker.is_none() => {
                if !app.suggestions.is_empty() {
                    let _ = app.accept_suggestion();
                } else if let Some(level) = app.cycle_effort(CycleDir::Right)
                    && let Some(conv) = conversation.as_mut()
                {
                    conv.set_effort(Some(level));
                }
            }
            KeyCode::Enter => {
                if let Some(picker) = app.model_picker.take() {
                    let result = picker.confirm();
                    apply_picker_result(&mut app, &mut conversation, result).await;
                    continue;
                }
                if app.busy {
                    continue;
                }
                // With suggestions open, Tab-style accept if input isn't a complete command yet.
                if !app.suggestions.is_empty() {
                    let typed = app.input.trim();
                    let exact = app
                        .suggestions
                        .items
                        .iter()
                        .any(|i| typed == format!("/{}", i.name));
                    if !exact {
                        app.accept_suggestion();
                        continue;
                    }
                }

                let prompt = app.input.trim().to_string();
                if prompt.is_empty() {
                    continue;
                }
                if matches!(prompt.as_str(), "/exit" | "/quit") {
                    app.should_quit = true;
                    continue;
                }

                // Slash commands — handle locally, do not send to the model.
                if let Some((name, args)) = parse_slash(&prompt) {
                    app.clear_input();
                    app.push(Role::User, prompt.clone());
                    match commands::dispatch(name, args, &app.model).await {
                        Ok(CommandAction::OpenModelPicker) => {
                            app.open_model_picker();
                        }
                        Ok(CommandAction::Message(outcome)) => {
                            if let Some(new_model) = outcome.new_model {
                                app.model = new_model.clone();
                                if let Some(conv) = conversation.as_mut()
                                    && let Err(e) = conv.set_model(&new_model).await
                                {
                                    app.push(Role::Error, format!("Failed to switch model: {e}"));
                                }
                            }
                            if let Some(effort) = outcome.new_effort {
                                app.effort = Some(effort);
                                if let Some(conv) = conversation.as_mut() {
                                    conv.set_effort(Some(effort));
                                }
                            }
                            app.push(Role::System, outcome.message);
                        }
                        Err(e) => {
                            app.push(Role::Error, e);
                        }
                    }
                    continue;
                }

                let Some(mut conv) = conversation.take() else {
                    app.push(Role::Error, "conversation unavailable");
                    continue;
                };

                app.clear_input();
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
                if !app.busy && app.model_picker.is_none() {
                    app.pop_input_char();
                }
            }
            KeyCode::Char(c) => {
                if app.model_picker.is_some() {
                    // j/k handled above; ignore other chars while picker is open
                    continue;
                }
                if !app.busy && !key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.push_input_char(c);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

async fn apply_picker_result(
    app: &mut App,
    conversation: &mut Option<Conversation>,
    result: PickerResult,
) {
    match result {
        PickerResult::Selected {
            message,
            new_model,
            effort,
        } => {
            app.model = new_model.clone();
            if let Some(e) = effort {
                app.effort = Some(e);
            }
            if let Some(conv) = conversation.as_mut() {
                if let Err(e) = conv.set_model(&new_model).await {
                    app.push(Role::Error, format!("Failed to switch model: {e}"));
                }
                if let Some(e) = effort {
                    conv.set_effort(Some(e));
                }
            }
            app.push(Role::System, message);
        }
        PickerResult::Cancelled { message } => {
            app.push(Role::System, message);
        }
    }
}
