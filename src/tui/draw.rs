//! Claude Code–style drawing (Ink silhouette: glyphs, hairlines, orange spinner).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::app::{App, Role};
use super::theme::{
    ASSISTANT_GLYPH, CLAUDE, CLAUDE_SHIMMER, ERROR, PROMPT_BORDER, PROMPT_GLYPH, SUBTLE, TEXT,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let suggest_h = if app.suggestions.is_empty() {
        0
    } else {
        (app.suggestions.items.len() as u16)
            .min(6)
            .saturating_add(0)
    };

    // History · suggestions · spinner · prompt · footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(2),
            Constraint::Length(suggest_h),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_history(frame, app, chunks[0]);
    if suggest_h > 0 {
        draw_suggestions(frame, app, chunks[1]);
    }
    draw_spinner(frame, app, chunks[2]);
    draw_prompt(frame, app, chunks[3]);
    draw_footer(frame, app, chunks[4]);

    if let Some(picker) = &app.model_picker {
        picker.draw(frame, frame.area());
    }
    if let Some(ui) = &app.pending_interaction {
        let area = super::ask_user::interaction_overlay_area(frame.area());
        super::ask_user::draw_interaction(frame, area, ui);
    }
}

fn draw_history(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    if app.history.is_empty() {
        lines.push(Line::from(Span::styled(
            "T5 Code",
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("  {}", short_model(&app.model)),
            Style::default().fg(SUBTLE),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Type a message and press Enter.",
            Style::default().fg(SUBTLE),
        )));
    }

    for msg in &app.history {
        match msg.role {
            Role::User => {
                let mut first = true;
                for line in msg.text.lines() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{PROMPT_GLYPH} "), Style::default().fg(SUBTLE)),
                            Span::styled(line.to_string(), Style::default().fg(TEXT)),
                        ]));
                        first = false;
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(line.to_string(), Style::default().fg(TEXT)),
                        ]));
                    }
                }
            }
            Role::Assistant => {
                let mut first = true;
                for line in msg.text.lines() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("{ASSISTANT_GLYPH} "),
                                Style::default().fg(CLAUDE),
                            ),
                            Span::styled(line.to_string(), Style::default().fg(TEXT)),
                        ]));
                        first = false;
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(line.to_string(), Style::default().fg(TEXT)),
                        ]));
                    }
                }
            }
            Role::System => {
                for line in msg.text.lines() {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(SUBTLE),
                    )));
                }
            }
            Role::Error => {
                let mut first = true;
                for line in msg.text.lines() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled(format!("{ASSISTANT_GLYPH} "), Style::default().fg(ERROR)),
                            Span::styled(line.to_string(), Style::default().fg(ERROR)),
                        ]));
                        first = false;
                    } else {
                        lines.push(Line::from(Span::styled(
                            format!("  {line}"),
                            Style::default().fg(ERROR),
                        )));
                    }
                }
            }
        }
        lines.push(Line::from(""));
    }

    let height = area.height as usize;
    let width = area.width as usize;
    // Truncate to width instead of Wrap — wrapping without accounting for
    // visual rows overflows the viewport and scrolls/corrupts the main buffer.
    let fitted: Vec<Line> = lines
        .into_iter()
        .map(|line| truncate_line(line, width))
        .collect();
    let total = fitted.len();
    let max_off = total.saturating_sub(height);
    let offset = app.history_offset.min(max_off);
    let start = total.saturating_sub(height + offset);
    let visible = fitted[start..].to_vec();

    // Borderless — Ink history has no panel chrome. Fill the area so stale
    // cells from previous frames don't linger on the main buffer.
    frame.render_widget(Paragraph::new(visible), area);
}

fn draw_suggestions(frame: &mut Frame, app: &App, area: Rect) {
    if app.suggestions.is_empty() || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let max = area.height as usize;
    for (i, item) in app.suggestions.items.iter().enumerate().take(max) {
        let selected = i == app.suggestions.selected;
        let pointer = if selected { "❯ " } else { "  " };
        let name_style = if selected {
            Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        let mut spans = vec![
            Span::styled(pointer, Style::default().fg(CLAUDE)),
            Span::styled(item.display.clone(), name_style),
        ];
        if !item.description.is_empty() {
            spans.push(Span::styled(
                format!("  {}", item.description),
                Style::default().fg(SUBTLE),
            ));
        }
        lines.push(truncate_line(Line::from(spans), width));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_spinner(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;
    if !app.busy {
        frame.render_widget(Paragraph::new(Line::from(" ".repeat(width))), area);
        return;
    }
    let line = Line::from(vec![
        Span::styled(
            format!("{} ", app.spinner_glyph()),
            Style::default().fg(CLAUDE),
        ),
        Span::styled(
            format!("{}…", app.spinner_verb),
            Style::default().fg(CLAUDE_SHIMMER),
        ),
    ]);
    frame.render_widget(Paragraph::new(truncate_line(line, width)), area);
}

fn draw_prompt(frame: &mut Frame, app: &App, area: Rect) {
    // Top + bottom hairlines only (Ink round border with left/right off).
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(PROMPT_BORDER));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let glyph_style = if app.busy {
        Style::default().fg(SUBTLE)
    } else {
        Style::default().fg(TEXT)
    };

    let body = if app.busy {
        Line::from(vec![
            Span::styled(format!("{PROMPT_GLYPH} "), glyph_style),
            Span::styled("", Style::default().fg(SUBTLE)),
        ])
    } else if app.input.is_empty() {
        Line::from(vec![
            Span::styled(format!("{PROMPT_GLYPH} "), glyph_style),
            Span::styled(app.placeholder(), Style::default().fg(SUBTLE)),
        ])
    } else {
        let cursor = if app.cursor_visible { "█" } else { " " };
        let mut spans = vec![
            Span::styled(format!("{PROMPT_GLYPH} "), glyph_style),
            Span::styled(app.input.clone(), Style::default().fg(TEXT)),
        ];
        if let Some(ghost) = app.ghost_suffix() {
            spans.push(Span::styled(ghost, Style::default().fg(SUBTLE)));
        }
        spans.push(Span::styled(cursor, Style::default().fg(TEXT)));
        Line::from(spans)
    };

    frame.render_widget(
        Paragraph::new(truncate_line(body, inner.width as usize)),
        inner,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let left = if app.busy {
        "esc to interrupt · ctrl+c to exit"
    } else if !app.suggestions.is_empty() {
        "tab to complete · ↑↓ select · esc dismiss"
    } else if app.displayed_effort().is_some() {
        "tab/shift+tab effort · ? shortcuts"
    } else {
        "? for shortcuts"
    };
    let right = {
        let model = short_model(&app.model);
        if let Some(e) = app.displayed_effort() {
            format!("{model} · {} {}", e.symbol(), e.as_str())
        } else {
            model
        }
    };
    let width = area.width as usize;
    let left_w = display_width(left);
    let right_w = display_width(&right);
    let spacer = width.saturating_sub(left_w).saturating_sub(right_w);
    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(SUBTLE)),
        Span::raw(" ".repeat(spacer)),
        Span::styled(right, Style::default().fg(SUBTLE)),
    ]);
    frame.render_widget(Paragraph::new(truncate_line(line, width)), area);
}

fn short_model(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).trim().to_string()
}

fn display_width(s: &str) -> usize {
    // Ambiguous-width chars count as 1 — good enough for footer padding.
    s.chars()
        .map(|c| if c.is_ascii() { 1 } else { unicode_width(c) })
        .sum()
}

fn unicode_width(c: char) -> usize {
    // Minimal width: most BMP symbols we use (❯ ● · ✢) are width 1.
    match c {
        '\u{1100}'..='\u{115f}'
        | '\u{2329}'..='\u{232a}'
        | '\u{2e80}'..='\u{a4cf}'
        | '\u{ac00}'..='\u{d7a3}'
        | '\u{f900}'..='\u{faff}'
        | '\u{fe10}'..='\u{fe19}'
        | '\u{fe30}'..='\u{fe6f}'
        | '\u{ff00}'..='\u{ff60}'
        | '\u{ffe0}'..='\u{ffe6}' => 2,
        _ => 1,
    }
}

fn truncate_line(line: Line<'_>, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }
    let mut used = 0usize;
    let mut spans: Vec<Span<'static>> = Vec::new();
    'outer: for span in line.spans {
        let mut out = String::new();
        for ch in span.content.chars() {
            let w = if ch.is_ascii() { 1 } else { unicode_width(ch) };
            if used + w > width {
                break 'outer;
            }
            out.push(ch);
            used += w;
        }
        if !out.is_empty() {
            spans.push(Span::styled(out, span.style));
        }
    }
    // Pad with spaces so the row fully overwrites previous cell contents.
    if used < width {
        spans.push(Span::raw(" ".repeat(width - used)));
    }
    Line::from(spans)
}
