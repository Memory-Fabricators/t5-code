//! AskUserQuestion + ExitPlanMode overlays.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tools::interaction::{PendingUi, UserPrompt, UserResponse};

use super::theme::{CLAUDE, SUBTLE};

pub fn draw_interaction(f: &mut Frame, area: Rect, ui: &PendingUi) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(match &ui.prompt {
            UserPrompt::AskUser { .. } => " Ask user ",
            UserPrompt::ExitPlan { .. } => " Approve plan? ",
        })
        .border_style(Style::default().fg(CLAUDE));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    match &ui.prompt {
        UserPrompt::AskUser { questions } => {
            draw_ask(f, inner, ui, questions);
        }
        UserPrompt::ExitPlan { plan, path } => {
            draw_exit(f, inner, ui, plan, path);
        }
    }
}

fn draw_ask(
    f: &mut Frame,
    area: Rect,
    ui: &PendingUi,
    questions: &[crate::tools::interaction::Question],
) {
    let q = match questions.get(ui.question_idx) {
        Some(q) => q,
        None => return,
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .split(area);

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("[{}/{}] {}", ui.question_idx + 1, questions.len(), q.header),
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD),
        )),
        Line::from(q.question.clone()),
    ])
    .wrap(Wrap { trim: true });
    f.render_widget(header, chunks[0]);

    let mut lines = Vec::new();
    for (i, opt) in q.options.iter().enumerate() {
        let selected = ui.selected.contains(&i);
        let marker = if selected { "▸" } else { " " };
        let style = if selected {
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{marker} {}. {} — {}", i + 1, opt.label, opt.description),
            style,
        )));
    }
    let other_marker = if ui.editing_other { "▸" } else { " " };
    lines.push(Line::from(format!(
        "{other_marker} O. Other: {}",
        if ui.other_text.is_empty() {
            "…"
        } else {
            &ui.other_text
        }
    )));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), chunks[1]);

    let hint = if q.multi_select {
        "Space toggle · Enter next/submit · Esc cancel · O other"
    } else {
        "1-4 select · Enter confirm · Esc cancel · O other"
    };
    f.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(SUBTLE))),
        chunks[2],
    );
}

fn draw_exit(f: &mut Frame, area: Rect, ui: &PendingUi, plan: &str, path: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(format!("Plan file: {path}")).style(Style::default().fg(SUBTLE)),
        chunks[0],
    );

    let preview: String = plan.chars().take(2000).collect();
    f.render_widget(
        Paragraph::new(preview).wrap(Wrap { trim: false }),
        chunks[1],
    );

    let approve = if ui.exit_choice == 0 {
        Span::styled(
            "▸ Approve",
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  Approve")
    };
    let reject = if ui.exit_choice == 1 {
        Span::styled(
            "▸ Reject",
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  Reject")
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            approve,
            Span::raw("   "),
            reject,
            Span::styled("   (←/→ · Enter · Esc)", Style::default().fg(SUBTLE)),
        ])),
        chunks[2],
    );
}

/// Handle a key while an interaction overlay is open. Returns true if consumed.
pub fn handle_key(ui: &mut PendingUi, code: ratatui::crossterm::event::KeyCode) -> bool {
    use ratatui::crossterm::event::KeyCode;

    match &ui.prompt.clone() {
        UserPrompt::AskUser { questions } => {
            let Some(q) = questions.get(ui.question_idx).cloned() else {
                return true;
            };
            if ui.editing_other {
                match code {
                    KeyCode::Esc => {
                        ui.editing_other = false;
                    }
                    KeyCode::Enter => {
                        let answer = ui.other_text.clone();
                        finish_question(ui, questions, &q.question, answer);
                    }
                    KeyCode::Backspace => {
                        ui.other_text.pop();
                    }
                    KeyCode::Char(c) => {
                        ui.other_text.push(c);
                    }
                    _ => {}
                }
                return true;
            }
            match code {
                KeyCode::Esc => {
                    complete(ui, UserResponse::Cancelled);
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    ui.editing_other = true;
                    ui.selected.clear();
                }
                KeyCode::Char(c @ '1'..='4') => {
                    let idx = (c as u8 - b'1') as usize;
                    if idx < q.options.len() {
                        if q.multi_select {
                            if let Some(pos) = ui.selected.iter().position(|x| *x == idx) {
                                ui.selected.remove(pos);
                            } else {
                                ui.selected.push(idx);
                            }
                        } else {
                            ui.selected = vec![idx];
                            let label = q.options[idx].label.clone();
                            finish_question(ui, questions, &q.question, label);
                        }
                    }
                }
                KeyCode::Char(' ') if q.multi_select => {
                    // toggle focused — use first unselected or cycle; treat as select 0 if empty
                }
                KeyCode::Enter => {
                    if q.multi_select {
                        if ui.selected.is_empty() {
                            return true;
                        }
                        let labels: Vec<String> = ui
                            .selected
                            .iter()
                            .filter_map(|i| q.options.get(*i).map(|o| o.label.clone()))
                            .collect();
                        finish_question(ui, questions, &q.question, labels.join(", "));
                    } else if let Some(&idx) = ui.selected.first() {
                        let label = q.options[idx].label.clone();
                        finish_question(ui, questions, &q.question, label);
                    }
                }
                KeyCode::Up | KeyCode::Down => {
                    // move selection highlight
                    let len = q.options.len();
                    if len == 0 {
                        return true;
                    }
                    let cur = ui.selected.first().copied().unwrap_or(0);
                    let next = if matches!(code, KeyCode::Up) {
                        cur.saturating_sub(1)
                    } else {
                        (cur + 1).min(len - 1)
                    };
                    if !q.multi_select {
                        ui.selected = vec![next];
                    }
                }
                _ => {}
            }
            true
        }
        UserPrompt::ExitPlan { .. } => {
            match code {
                KeyCode::Esc => complete(ui, UserResponse::Cancelled),
                KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                    ui.exit_choice = 1 - ui.exit_choice;
                }
                KeyCode::Enter => {
                    let approved = ui.exit_choice == 0;
                    complete(ui, UserResponse::ExitPlan { approved });
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    complete(ui, UserResponse::ExitPlan { approved: true });
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    complete(ui, UserResponse::ExitPlan { approved: false });
                }
                _ => {}
            }
            true
        }
    }
}

fn finish_question(
    ui: &mut PendingUi,
    questions: &[crate::tools::interaction::Question],
    question_text: &str,
    answer: String,
) {
    ui.answers.insert(question_text.to_string(), answer);
    ui.selected.clear();
    ui.other_text.clear();
    ui.editing_other = false;
    if ui.question_idx + 1 < questions.len() {
        ui.question_idx += 1;
    } else {
        let answers = std::mem::take(&mut ui.answers);
        complete(ui, UserResponse::AskUser { answers });
    }
}

fn complete(ui: &mut PendingUi, response: UserResponse) {
    if let Some(tx) = ui.response_tx.take() {
        let _ = tx.send(response);
    }
}

pub fn interaction_overlay_area(frame: Rect) -> Rect {
    let w = frame.width.saturating_mul(4) / 5;
    let h = frame.height.saturating_mul(3) / 4;
    let x = frame.x + (frame.width.saturating_sub(w)) / 2;
    let y = frame.y + (frame.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w.max(40),
        height: h.max(12),
    }
}
