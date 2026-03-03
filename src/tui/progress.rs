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

//! Progress dialog — set task completion percentage via a TUI popup.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Widget};

use crate::tui::theme::Theme;

/// Secondary text style — visible gray foreground.
const SECONDARY: Style = Style::new().fg(Color::Gray);

/// Bar width in cells for the progress gauge.
const BAR_WIDTH: usize = 20;

/// State for the progress dialog overlay.
pub struct ProgressDialogState {
    pub task_id: String,
    pub task_title: String,
    input: String,
    cursor: usize,
}

impl ProgressDialogState {
    /// Create a progress dialog pre-filled with the current value.
    pub fn new(task_id: String, task_title: String, current: Option<u8>) -> Self {
        let input = current.map(|v| v.to_string()).unwrap_or_default();
        let cursor = input.len();
        Self {
            task_id,
            task_title,
            input,
            cursor,
        }
    }

    /// Parse the current input as a valid progress value (0–100).
    pub fn value(&self) -> Option<u8> {
        let v: u16 = self.input.parse().ok()?;
        if v <= 100 { Some(v as u8) } else { None }
    }

    /// Insert a character at the cursor position.
    pub fn char_input(&mut self, c: char) {
        if !c.is_ascii_digit() {
            return;
        }
        // Limit to 3 digits (max "100").
        if self.input.len() >= 3 {
            return;
        }
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    /// Adjust the current value by `delta` (clamped to 0–100).
    pub fn adjust(&mut self, delta: i16) {
        let current = self.value().unwrap_or(0) as i16;
        let new = current.saturating_add(delta).clamp(0, 100) as u8;
        self.input = new.to_string();
        self.cursor = self.input.len();
    }

    /// Whether the input is empty.
    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    /// Render the progress dialog centred on the screen.
    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup = centered_rect(area, 44, 10);
        Clear.render(popup, buf);

        let block = Block::bordered()
            .title(" progress ")
            .border_style(theme.border_focused)
            .padding(Padding::new(1, 1, 1, 1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([
            Constraint::Length(1), // task title
            Constraint::Length(1), // gap
            Constraint::Length(1), // bar + input
            Constraint::Fill(1),   // spacer
            Constraint::Length(1), // hints
        ])
        .split(inner);

        // Title: truncated task title
        let max_title_len = inner.width.saturating_sub(2) as usize;
        let display_title = if self.task_title.len() > max_title_len {
            format!("{}…", &self.task_title[..max_title_len.saturating_sub(1)])
        } else {
            self.task_title.clone()
        };
        Line::from(Span::styled(
            format!(" {display_title}"),
            theme.accent.add_modifier(Modifier::BOLD),
        ))
        .render(rows[0], buf);

        // Progress bar + input field
        let pct = self.value().unwrap_or(0);
        let mut spans = vec![Span::raw(" ")];
        spans.extend(gauge_spans(pct));
        spans.push(Span::raw("  "));

        // Input box: [___] %
        let display_input = format!("{:<3}", self.input);
        spans.push(Span::styled(
            format!("[{display_input}]"),
            theme.accent.add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" %"));

        Line::from(spans).render(rows[2], buf);

        // Hints
        let hints = Line::from(vec![
            Span::styled(" \u{2190}/\u{2192}", theme.accent),
            Span::styled(" \u{00b1}10", SECONDARY),
            Span::styled("  Enter", theme.accent),
            Span::styled(" save", SECONDARY),
            Span::styled("  Esc", theme.accent),
            Span::styled(" cancel", SECONDARY),
        ]);
        hints.render(rows[4], buf);
    }
}

/// Build gauge spans for a percentage value (0–100) using a fixed-width bar.
fn gauge_spans(pct: u8) -> Vec<Span<'static>> {
    let filled = (pct as usize * BAR_WIDTH) / 100;
    let empty = BAR_WIDTH - filled;
    let color = match pct {
        0..=49 => Color::Yellow,
        50..=99 => Color::Cyan,
        _ => Color::Green,
    };

    let mut spans = Vec::new();
    if filled > 0 {
        spans.push(Span::styled(
            "\u{2588}".repeat(filled),
            Style::new().fg(color),
        ));
    }
    if empty > 0 {
        spans.push(Span::styled("\u{00b7}".repeat(empty), SECONDARY));
    }
    spans
}

/// Compute a centred popup rectangle.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);

    let vertical = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
