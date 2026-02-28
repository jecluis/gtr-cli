// SPDX-License-Identifier: AGPL-3.0-or-later
// gtr - CLI client for Getting Things Rusty
// Copyright (C) 2026 Joao Eduardo Luis <joao@abysmo.tech>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Centred overlay form for creating new tasks.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Widget};

use crate::tui::theme::Theme;

const SIZES: [&str; 4] = ["S", "M", "L", "XL"];

/// Which field has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FormField {
    Title,
    Priority,
    Size,
}

/// State for the task creation form overlay.
pub struct CreateFormState {
    pub project_id: String,
    pub project_name: String,
    title: String,
    cursor_pos: usize,
    priority: String,
    size_idx: usize,
    focused: FormField,
}

impl CreateFormState {
    pub fn new(project_id: String, project_name: String) -> Self {
        Self {
            project_id,
            project_name,
            title: String::new(),
            cursor_pos: 0,
            priority: "later".to_string(),
            size_idx: 1, // default M
            focused: FormField::Title,
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn priority(&self) -> &str {
        &self.priority
    }

    pub fn size(&self) -> &str {
        SIZES[self.size_idx]
    }

    /// Whether the title is non-empty (ready to submit).
    pub fn can_submit(&self) -> bool {
        !self.title.trim().is_empty()
    }

    /// Move focus to the next field.
    pub fn focus_next(&mut self) {
        self.focused = match self.focused {
            FormField::Title => FormField::Priority,
            FormField::Priority => FormField::Size,
            FormField::Size => FormField::Title,
        };
    }

    /// Move focus to the previous field.
    pub fn focus_prev(&mut self) {
        self.focused = match self.focused {
            FormField::Title => FormField::Size,
            FormField::Priority => FormField::Title,
            FormField::Size => FormField::Priority,
        };
    }

    /// Handle character input (only affects title field).
    pub fn char_input(&mut self, c: char) {
        if self.focused == FormField::Title {
            self.title.insert(self.cursor_pos, c);
            self.cursor_pos += c.len_utf8();
        }
    }

    /// Handle backspace.
    pub fn backspace(&mut self) {
        if self.focused == FormField::Title && self.cursor_pos > 0 {
            // Find the previous char boundary
            let prev = self.title[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.title.remove(prev);
            self.cursor_pos = prev;
        }
    }

    /// Handle space or toggle for non-title fields.
    pub fn toggle_or_space(&mut self) {
        match self.focused {
            FormField::Title => self.char_input(' '),
            FormField::Priority => {
                self.priority = if self.priority == "now" {
                    "later".to_string()
                } else {
                    "now".to_string()
                };
            }
            FormField::Size => {
                self.size_idx = (self.size_idx + 1) % SIZES.len();
            }
        }
    }

    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup_width = 52u16.min(area.width.saturating_sub(4));
        let popup_height = 11;
        let popup = centered_rect(popup_width, popup_height, area);

        Clear.render(popup, buf);

        let border_style = theme.accent;
        let block = Block::bordered()
            .title(format!(" new task in {} ", self.project_name))
            .border_style(border_style);
        let inner = block.inner(popup);
        block.render(popup, buf);

        let field_rows = Layout::vertical([
            Constraint::Length(1), // padding
            Constraint::Length(1), // title label + input
            Constraint::Length(1), // title input
            Constraint::Length(1), // padding
            Constraint::Length(1), // priority
            Constraint::Length(1), // size
            Constraint::Length(1), // padding
            Constraint::Length(1), // submit hint
        ])
        .split(inner);

        // Title field
        let title_style = if self.focused == FormField::Title {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        Line::from(vec![Span::raw("  "), Span::styled("Title: ", title_style)])
            .render(field_rows[1], buf);

        // Title input with cursor
        let display_title = if self.title.is_empty() && self.focused == FormField::Title {
            "\u{2588}" // block cursor
        } else if self.focused == FormField::Title {
            // Show text with cursor
            &self.title
        } else {
            &self.title
        };

        let mut title_spans = vec![Span::raw("  ")];
        if self.focused == FormField::Title && !self.title.is_empty() {
            let (before, after) = self.title.split_at(self.cursor_pos);
            title_spans.push(Span::raw(before.to_string()));
            title_spans.push(Span::styled(
                if after.is_empty() {
                    "\u{2588}".to_string()
                } else {
                    after.chars().next().unwrap().to_string()
                },
                theme.selected,
            ));
            if after.len() > 1 {
                title_spans.push(Span::raw(
                    after[after.chars().next().unwrap().len_utf8()..].to_string(),
                ));
            }
        } else {
            title_spans.push(Span::raw(display_title.to_string()));
        }
        Line::from(title_spans).render(field_rows[2], buf);

        // Priority field
        let pri_style = if self.focused == FormField::Priority {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        let now_style = if self.priority == "now" {
            theme.danger.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        let later_style = if self.priority == "later" {
            theme.success.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Priority: ", pri_style),
            Span::styled("now", now_style),
            Span::raw(" / "),
            Span::styled("later", later_style),
        ])
        .render(field_rows[4], buf);

        // Size field
        let size_style = if self.focused == FormField::Size {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        let mut size_spans = vec![Span::raw("  "), Span::styled("Size:     ", size_style)];
        for (i, s) in SIZES.iter().enumerate() {
            let style = if i == self.size_idx {
                theme.accent.add_modifier(Modifier::BOLD)
            } else {
                theme.muted
            };
            if i > 0 {
                size_spans.push(Span::raw(" / "));
            }
            size_spans.push(Span::styled(*s, style));
        }
        Line::from(size_spans).render(field_rows[5], buf);

        // Submit hint
        let hint = if self.can_submit() {
            Line::from(vec![
                Span::styled("  Enter", theme.status_key),
                Span::styled(" create", theme.status_desc),
                Span::styled("  Esc", theme.status_key),
                Span::styled(" cancel", theme.status_desc),
            ])
        } else {
            Line::from(vec![
                Span::styled("  type a title to create", theme.muted),
                Span::styled("  Esc", theme.status_key),
                Span::styled(" cancel", theme.status_desc),
            ])
        };
        hint.alignment(Alignment::Left).render(field_rows[7], buf);
    }
}

/// Return a centred `Rect` of the given width and height within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(v[1]);
    h[1]
}
