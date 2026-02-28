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

//! Centred yes/no confirmation dialog overlay for destructive actions.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::tui::theme::Theme;

/// The destructive action awaiting confirmation.
pub enum PendingAction {
    Done {
        task_id: String,
        title: String,
        descendant_count: usize,
    },
    Delete {
        task_id: String,
        title: String,
        child_count: usize,
    },
}

/// Which button is highlighted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    No,
    Yes,
}

/// State for the confirmation dialog overlay.
pub struct ConfirmState {
    pub action: PendingAction,
    selected: Choice,
}

impl ConfirmState {
    pub fn new(action: PendingAction) -> Self {
        Self {
            action,
            selected: Choice::No, // default to No (safe)
        }
    }

    pub fn toggle(&mut self) {
        self.selected = match self.selected {
            Choice::No => Choice::Yes,
            Choice::Yes => Choice::No,
        };
    }

    pub fn is_confirmed(&self) -> bool {
        self.selected == Choice::Yes
    }

    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let (title, message, warning) = match &self.action {
            PendingAction::Done {
                title,
                descendant_count,
                ..
            } => {
                let warn = if *descendant_count > 0 {
                    format!("{} subtask(s) will also be marked done.", descendant_count)
                } else {
                    String::new()
                };
                ("Mark as done?", format!("\"{}\"", title), warn)
            }
            PendingAction::Delete {
                title, child_count, ..
            } => {
                let warn = if *child_count > 0 {
                    format!("{} child(ren) will be promoted to parent.", child_count)
                } else {
                    String::new()
                };
                ("Delete task?", format!("\"{}\"", title), warn)
            }
        };

        // Compute popup dimensions
        let popup_width = 50u16.min(area.width.saturating_sub(4));
        let popup_height = if warning.is_empty() { 7 } else { 9 };
        let popup = centered_rect(popup_width, popup_height, area);

        // Clear the area behind the popup
        Clear.render(popup, buf);

        // Build content lines
        let mut lines = vec![
            Line::default(),
            Line::from(message).alignment(Alignment::Center),
        ];

        if !warning.is_empty() {
            lines.push(Line::default());
            lines.push(
                Line::from(Span::styled(warning, theme.warning)).alignment(Alignment::Center),
            );
        }

        lines.push(Line::default());

        // Buttons
        let no_style = if self.selected == Choice::No {
            theme.selected.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        let yes_style = if self.selected == Choice::Yes {
            theme.danger.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };

        lines.push(
            Line::from(vec![
                Span::raw("    "),
                Span::styled(" No ", no_style),
                Span::raw("   "),
                Span::styled(" Yes ", yes_style),
                Span::raw("    "),
            ])
            .alignment(Alignment::Center),
        );

        let border_style = theme.danger;
        let block = Block::bordered()
            .title(format!(" {} ", title))
            .border_style(border_style);

        Paragraph::new(lines).block(block).render(popup, buf);
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
