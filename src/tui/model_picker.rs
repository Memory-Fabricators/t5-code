//! Interactive model picker — port of `src/components/ModelPicker.tsx` (list UI + effort).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::utils::effort::{
    CycleDir, EffortLevel, cycle_effort, get_default_effort_level_for_model, get_session_effort,
    model_supports_effort, model_supports_max_effort, set_session_effort,
};
use crate::utils::model::{
    get_default_main_loop_model, parse_user_specified_model, render_model_label, render_model_name,
    set_session_model,
};
use crate::utils::model_options::{ModelOption, NO_PREFERENCE, get_model_options, selection_key};

use super::theme::{CLAUDE, PERMISSION, REMEMBER, SUBTLE, TEXT};

const HEADER: &str = "Switch between Claude models. Applies to this session and future Claude Code sessions. For other/previous model names, specify with --model.";
const VISIBLE: usize = 10;

#[derive(Debug, Clone)]
pub struct ModelPicker {
    options: Vec<ModelOption>,
    /// Index into `options`.
    focused: usize,
    /// Scroll offset into `options` for the visible window.
    scroll: usize,
    /// Current effort selection (cycled with ← →).
    effort: EffortLevel,
    /// User pressed ←/→ at least once (persist / show in confirm message).
    has_toggled_effort: bool,
}

#[derive(Debug, Clone)]
pub enum PickerResult {
    Selected {
        message: String,
        new_model: String,
        effort: Option<EffortLevel>,
    },
    Cancelled {
        message: String,
    },
}

impl ModelPicker {
    pub fn open(current_model: &str) -> Self {
        let options = get_model_options(current_model);
        let key = selection_key(current_model);
        let focused = options.iter().position(|o| o.value == key).unwrap_or(0);
        let focused_model = resolve_option_model(&options[focused].value);
        let effort = get_session_effort()
            .unwrap_or_else(|| get_default_effort_level_for_model(&focused_model));
        let mut picker = Self {
            options,
            focused,
            scroll: 0,
            effort,
            has_toggled_effort: false,
        };
        picker.ensure_visible();
        picker
    }

    pub fn move_up(&mut self) {
        if self.focused > 0 {
            self.focused -= 1;
            self.on_focus_change();
            self.ensure_visible();
        }
    }

    pub fn move_down(&mut self) {
        if self.focused + 1 < self.options.len() {
            self.focused += 1;
            self.on_focus_change();
            self.ensure_visible();
        }
    }

    pub fn cycle_effort_left(&mut self) {
        self.cycle_effort(CycleDir::Left);
    }

    pub fn cycle_effort_right(&mut self) {
        self.cycle_effort(CycleDir::Right);
    }

    fn cycle_effort(&mut self, dir: CycleDir) {
        let model = self.focused_resolved_model();
        if !model_supports_effort(&model) {
            return;
        }
        let include_max = model_supports_max_effort(&model);
        let base = if self.effort == EffortLevel::Max && !include_max {
            EffortLevel::High
        } else {
            self.effort
        };
        self.effort = cycle_effort(base, dir, include_max);
        self.has_toggled_effort = true;
    }

    fn on_focus_change(&mut self) {
        if !self.has_toggled_effort {
            let model = self.focused_resolved_model();
            self.effort = get_default_effort_level_for_model(&model);
        }
    }

    fn focused_resolved_model(&self) -> String {
        resolve_option_model(&self.options[self.focused].value)
    }

    fn ensure_visible(&mut self) {
        let visible = VISIBLE.min(self.options.len());
        if self.focused < self.scroll {
            self.scroll = self.focused;
        } else if self.focused >= self.scroll + visible {
            self.scroll = self.focused + 1 - visible;
        }
    }

    pub fn confirm(&self) -> PickerResult {
        let opt = &self.options[self.focused];
        let (setting, resolved) = if opt.value == NO_PREFERENCE {
            set_session_model(None);
            (None, get_default_main_loop_model())
        } else {
            let alias = opt.value.clone();
            let resolved = parse_user_specified_model(&alias);
            set_session_model(Some(alias.clone()));
            (Some(alias), resolved)
        };

        let supports = model_supports_effort(&resolved);
        let selected_effort = if self.has_toggled_effort && supports {
            Some(self.effort)
        } else {
            None
        };
        if let Some(e) = selected_effort {
            set_session_effort(Some(e));
        }

        let mut message = format!(
            "Set model to {}",
            match &setting {
                None => render_model_label(None),
                Some(a) => render_model_name(a),
            }
        );
        if let Some(e) = selected_effort {
            message.push_str(&format!(" with {} effort", e.as_str()));
        }

        PickerResult::Selected {
            message,
            new_model: resolved,
            effort: selected_effort,
        }
    }

    pub fn cancel(&self, current_model: &str) -> PickerResult {
        PickerResult::Cancelled {
            message: format!("Kept model as {}", render_model_name(current_model)),
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.min(72).max(40.min(area.width));
        let list_h = VISIBLE.min(self.options.len()) as u16;
        let height = (list_h + 11).min(area.height.saturating_sub(2)).max(10);
        let x = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        let pane = Rect {
            x,
            y,
            width,
            height,
        };

        frame.render_widget(Clear, pane);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PERMISSION))
            .title(Span::styled(
                " Select model ",
                Style::default().fg(REMEMBER).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(pane);
        frame.render_widget(block, pane);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // header
                Constraint::Min(3),    // list
                Constraint::Length(1), // effort
                Constraint::Length(1), // hints
            ])
            .split(inner);

        let header = Paragraph::new(vec![Line::from(Span::styled(
            HEADER,
            Style::default().fg(SUBTLE),
        ))])
        .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(header, chunks[0]);

        let visible = VISIBLE.min(self.options.len());
        let hidden = self.options.len().saturating_sub(visible);
        let mut lines: Vec<Line> = Vec::new();
        for (i, opt) in self
            .options
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(visible)
        {
            let selected = i == self.focused;
            let pointer = if selected { "❯ " } else { "  " };
            let label_style = if selected {
                Style::default().fg(CLAUDE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            lines.push(Line::from(vec![
                Span::styled(pointer, Style::default().fg(CLAUDE)),
                Span::styled(opt.label.clone(), label_style),
            ]));
            if selected {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(opt.description.clone(), Style::default().fg(SUBTLE)),
                ]));
            }
        }
        if hidden > 0 && self.scroll + visible < self.options.len() {
            lines.push(Line::from(Span::styled(
                format!("  and {hidden} more…"),
                Style::default().fg(SUBTLE),
            )));
        }
        frame.render_widget(Paragraph::new(lines), chunks[1]);

        let focused_model = self.focused_resolved_model();
        let supports = model_supports_effort(&focused_model);
        let display_effort =
            if self.effort == EffortLevel::Max && !model_supports_max_effort(&focused_model) {
                EffortLevel::High
            } else {
                self.effort
            };
        let default_effort = get_default_effort_level_for_model(&focused_model);
        let effort_line = if supports {
            let default_tag = if display_effort == default_effort && !self.has_toggled_effort {
                " (default)"
            } else {
                ""
            };
            Line::from(vec![
                Span::styled(
                    format!("{} ", display_effort.symbol()),
                    Style::default().fg(CLAUDE),
                ),
                Span::styled(
                    format!("{} effort{default_tag} ", display_effort.display_name()),
                    Style::default().fg(SUBTLE),
                ),
                Span::styled("← → to adjust", Style::default().fg(SUBTLE)),
            ])
        } else {
            let name = self.options[self.focused].label.clone();
            Line::from(Span::styled(
                format!("○ Effort not supported for {name}"),
                Style::default().fg(SUBTLE),
            ))
        };
        frame.render_widget(Paragraph::new(effort_line), chunks[2]);

        let hints = Line::from(vec![
            Span::styled(
                "Enter",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm", Style::default().fg(SUBTLE)),
            Span::styled(" · ", Style::default().fg(SUBTLE)),
            Span::styled(
                "Esc",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" exit", Style::default().fg(SUBTLE)),
            Span::styled(" · ", Style::default().fg(SUBTLE)),
            Span::styled("↑↓", Style::default().fg(TEXT)),
            Span::styled(" navigate", Style::default().fg(SUBTLE)),
        ]);
        frame.render_widget(Paragraph::new(hints), chunks[3]);
    }
}

fn resolve_option_model(value: &str) -> String {
    if value == NO_PREFERENCE {
        get_default_main_loop_model()
    } else {
        parse_user_specified_model(value)
    }
}
