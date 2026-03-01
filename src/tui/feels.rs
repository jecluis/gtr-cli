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

//! Feels dialog — set daily energy and focus via a TUI popup.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Widget};

use crate::display::{energy_description, focus_description};
use crate::tui::theme::Theme;

/// Secondary text style — visible gray foreground instead of DIM modifier.
const SECONDARY: Style = Style::new().fg(Color::Gray);

/// Which field is focused in the feels dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FeelsField {
    Energy,
    Focus,
}

/// State for the feels dialog overlay.
pub struct FeelsDialogState {
    pub energy: u8,
    pub focus: u8,
    field: FeelsField,
}

impl Default for FeelsDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl FeelsDialogState {
    /// Create a new feels dialog with defaults (3, 3).
    pub fn new() -> Self {
        Self {
            energy: 3,
            focus: 3,
            field: FeelsField::Energy,
        }
    }

    /// Create a feels dialog pre-filled with existing values.
    pub fn with_values(energy: u8, focus: u8) -> Self {
        Self {
            energy: energy.clamp(1, 5),
            focus: focus.clamp(1, 5),
            field: FeelsField::Energy,
        }
    }

    /// Move to the next field.
    pub fn next_field(&mut self) {
        self.field = match self.field {
            FeelsField::Energy => FeelsField::Focus,
            FeelsField::Focus => FeelsField::Energy,
        };
    }

    /// Move to the previous field.
    pub fn prev_field(&mut self) {
        self.next_field(); // Only two fields, so same as next.
    }

    /// Increase the current field's value.
    pub fn increment(&mut self) {
        match self.field {
            FeelsField::Energy => self.energy = (self.energy + 1).min(5),
            FeelsField::Focus => self.focus = (self.focus + 1).min(5),
        }
    }

    /// Decrease the current field's value.
    pub fn decrement(&mut self) {
        match self.field {
            FeelsField::Energy => self.energy = self.energy.saturating_sub(1).max(1),
            FeelsField::Focus => self.focus = self.focus.saturating_sub(1).max(1),
        }
    }

    /// Render the feels dialog centred on the screen.
    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup = centered_rect(area, 55, 10);
        Clear.render(popup, buf);

        let block = Block::bordered()
            .title(" feels ")
            .border_style(theme.border_focused)
            .padding(Padding::new(1, 1, 1, 1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([
            Constraint::Length(1), // Energy row
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Focus row
            Constraint::Fill(1),   // spacer before hints
            Constraint::Length(1), // hints
        ])
        .split(inner);

        let energy_focused = self.field == FeelsField::Energy;
        let focus_focused = self.field == FeelsField::Focus;

        render_field(
            "Energy",
            self.energy,
            energy_description,
            energy_focused,
            theme,
            rows[0],
            buf,
        );
        render_field(
            " Focus",
            self.focus,
            focus_description,
            focus_focused,
            theme,
            rows[2],
            buf,
        );

        // Hints
        let hints = Line::from(vec![
            Span::styled(" Tab", theme.accent),
            Span::styled(" switch", SECONDARY),
            Span::styled("  \u{2190}\u{2192}", theme.accent),
            Span::styled(" adjust", SECONDARY),
            Span::styled("  Enter", theme.accent),
            Span::styled(" save", SECONDARY),
            Span::styled("  Esc", theme.accent),
            Span::styled(" cancel", SECONDARY),
        ]);
        hints.render(rows[4], buf);
    }
}

/// Render a single feels field as: `  Label: █████  description`.
fn render_field(
    name: &str,
    value: u8,
    describe: fn(u8) -> &'static str,
    focused: bool,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let label_style = if focused {
        theme.accent.add_modifier(Modifier::BOLD)
    } else {
        SECONDARY
    };
    let desc_style = if focused { Style::default() } else { SECONDARY };

    let mut spans = vec![Span::styled(format!(" {name}: "), label_style)];
    spans.extend(gauge_spans(value));
    spans.push(Span::styled(format!("  {}", describe(value)), desc_style));

    Line::from(spans).render(area, buf);
}

/// Build gauge spans: filled blocks + empty dots, colored by value.
fn gauge_spans(value: u8) -> Vec<Span<'static>> {
    let filled = value.min(5) as usize;
    let empty = 5 - filled;
    let color = match value {
        4..=5 => Color::Green,
        2..=3 => Color::Yellow,
        _ => Color::Red,
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
