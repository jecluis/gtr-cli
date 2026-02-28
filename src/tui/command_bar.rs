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

//! Bottom-line command bar activated by `:`, similar to vim.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;

/// Parsed command from the command bar.
pub enum Command {
    /// Create a task with the given title (defaults: later priority, M size).
    New { title: String },
    /// Quit the TUI.
    Quit,
    /// Unrecognised command.
    Unknown(String),
}

/// State for the command bar input.
pub struct CommandBarState {
    input: String,
    cursor_pos: usize,
}

impl Default for CommandBarState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandBarState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn char_input(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.input[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor_pos = prev;
        }
    }

    /// Whether the input is empty (should cancel on backspace past start).
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    /// Parse the current input into a command.
    pub fn parse(&self) -> Command {
        let trimmed = self.input.trim();

        if trimmed == "q" || trimmed == "quit" {
            return Command::Quit;
        }

        if let Some(rest) = trimmed.strip_prefix("new ") {
            let title = rest.trim().to_string();
            if !title.is_empty() {
                return Command::New { title };
            }
        }

        Command::Unknown(trimmed.to_string())
    }

    /// Render the command bar at the bottom of the screen.
    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![Span::styled(":", theme.accent.add_modifier(Modifier::BOLD))];

        if !self.input.is_empty() {
            let (before, after) = self.input.split_at(self.cursor_pos);
            spans.push(Span::raw(before.to_string()));
            if after.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
            } else {
                let first_char = after.chars().next().unwrap();
                spans.push(Span::styled(first_char.to_string(), theme.selected));
                if after.len() > first_char.len_utf8() {
                    spans.push(Span::raw(after[first_char.len_utf8()..].to_string()));
                }
            }
        } else {
            spans.push(Span::styled("\u{2588}", theme.selected));
        }

        Line::from(spans).style(theme.status_bar).render(area, buf);
    }
}
