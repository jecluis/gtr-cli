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

//! Lightweight two-field overlay for relocating a document.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Widget};

use crate::tui::theme::Theme;

/// Which field has focus in the move dialog.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MoveFormField {
    Namespace,
    Parent,
    Move,
    Cancel,
}

/// State for the document move overlay dialog.
pub struct MoveFormState {
    doc_id: String,
    doc_title: String,
    current_namespace_name: String,
    focused: MoveFormField,
    // Namespace field
    namespace_input: String,
    namespace_cursor: usize,
    resolved_namespace_id: Option<String>,
    resolved_namespace_name: Option<String>,
    // Parent field
    parent_input: String,
    parent_cursor: usize,
    resolved_parent_id: Option<String>,
    resolved_parent_title: Option<String>,
}

/// Fields that cycle with Tab/BackTab (excludes button row).
const FIELDS: &[MoveFormField] = &[MoveFormField::Namespace, MoveFormField::Parent];

impl MoveFormState {
    /// Create a new move form for the given document.
    pub fn new(doc_id: String, doc_title: String, current_namespace_name: String) -> Self {
        Self {
            doc_id,
            doc_title,
            current_namespace_name,
            focused: MoveFormField::Namespace,
            namespace_input: String::new(),
            namespace_cursor: 0,
            resolved_namespace_id: None,
            resolved_namespace_name: None,
            parent_input: String::new(),
            parent_cursor: 0,
            resolved_parent_id: None,
            resolved_parent_title: None,
        }
    }

    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }

    /// At least one field must be resolved for the move to make sense.
    pub fn can_submit(&self) -> bool {
        self.resolved_namespace_id.is_some()
            || !self.parent_input.is_empty()
            || self.resolved_parent_id.is_some()
    }

    pub fn namespace_input(&self) -> &str {
        &self.namespace_input
    }

    pub fn parent_input(&self) -> &str {
        &self.parent_input
    }

    pub fn resolved_namespace_id(&self) -> Option<&str> {
        self.resolved_namespace_id.as_deref()
    }

    pub fn resolved_parent_id(&self) -> Option<&str> {
        self.resolved_parent_id.as_deref()
    }

    /// Whether the parent field was explicitly cleared (empty input means
    /// un-parent).
    pub fn parent_cleared(&self) -> bool {
        self.parent_input.is_empty()
    }

    pub fn is_namespace_focused(&self) -> bool {
        self.focused == MoveFormField::Namespace
    }

    pub fn is_parent_focused(&self) -> bool {
        self.focused == MoveFormField::Parent
    }

    /// Move focus to the next field.
    pub fn focus_next(&mut self) {
        match self.focused {
            MoveFormField::Cancel => self.focused = FIELDS[0],
            MoveFormField::Move => self.focused = MoveFormField::Cancel,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos + 1 < FIELDS.len() {
                        self.focused = FIELDS[pos + 1];
                    } else {
                        self.focused = MoveFormField::Move;
                    }
                }
            }
        }
    }

    /// Move focus to the previous field.
    pub fn focus_prev(&mut self) {
        match self.focused {
            MoveFormField::Move => self.focused = *FIELDS.last().unwrap(),
            MoveFormField::Cancel => self.focused = MoveFormField::Move,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos > 0 {
                        self.focused = FIELDS[pos - 1];
                    } else {
                        self.focused = MoveFormField::Cancel;
                    }
                }
            }
        }
    }

    /// Handle character input for the focused text field.
    pub fn char_input(&mut self, c: char) {
        match self.focused {
            MoveFormField::Namespace => {
                self.namespace_input.insert(self.namespace_cursor, c);
                self.namespace_cursor += c.len_utf8();
            }
            MoveFormField::Parent => {
                self.parent_input.insert(self.parent_cursor, c);
                self.parent_cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    /// Handle backspace for the focused text field.
    pub fn backspace(&mut self) {
        match self.focused {
            MoveFormField::Namespace => {
                if self.namespace_cursor > 0 {
                    let prev = self.namespace_input[..self.namespace_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.namespace_input.remove(prev);
                    self.namespace_cursor = prev;
                }
            }
            MoveFormField::Parent => {
                if self.parent_cursor > 0 {
                    let prev = self.parent_input[..self.parent_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.parent_input.remove(prev);
                    self.parent_cursor = prev;
                }
            }
            _ => {}
        }
    }

    /// Whether the Move button currently has focus.
    pub fn is_move_focused(&self) -> bool {
        self.focused == MoveFormField::Move
    }

    /// Whether the Cancel button currently has focus.
    pub fn is_cancel_focused(&self) -> bool {
        self.focused == MoveFormField::Cancel
    }

    /// Set the resolved namespace.
    pub fn set_resolved_namespace(&mut self, id: Option<String>, name: Option<String>) {
        self.resolved_namespace_id = id;
        self.resolved_namespace_name = name;
    }

    /// Set the resolved parent document.
    pub fn set_resolved_parent(&mut self, id: Option<String>, title: Option<String>) {
        self.resolved_parent_id = id;
        self.resolved_parent_title = title;
    }

    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup_width = 50u16.min(area.width.saturating_sub(4));
        let popup_height = 10;
        let popup = centered_rect(popup_width, popup_height, area);

        Clear.render(popup, buf);

        let border_style = theme.accent;
        let truncated_title = if self.doc_title.chars().count() > 28 {
            let truncated: String = self.doc_title.chars().take(25).collect();
            format!("{truncated}...")
        } else {
            self.doc_title.clone()
        };
        let title = format!(" Move: {truncated_title} ");
        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([
            Constraint::Length(1), // Namespace label + input
            Constraint::Length(1), // padding
            Constraint::Length(1), // Parent label + input
            Constraint::Length(1), // padding
            Constraint::Length(1), // padding
            Constraint::Length(1), // padding
            Constraint::Length(1), // padding
            Constraint::Length(1), // buttons
        ])
        .split(inner);

        // Namespace field
        self.render_namespace_field(theme, rows[0], buf);

        // Parent field
        self.render_parent_field(theme, rows[2], buf);

        // Move/Cancel buttons
        self.render_buttons(theme, rows[7], buf);
    }

    fn render_namespace_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let label_style = field_label_style(self.focused == MoveFormField::Namespace, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Namespace:  ", label_style)];

        if self.focused == MoveFormField::Namespace {
            if self.namespace_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                spans.push(Span::styled(
                    format!(" ({})", self.current_namespace_name),
                    theme.muted,
                ));
            } else {
                render_cursor_spans(
                    &self.namespace_input,
                    self.namespace_cursor,
                    theme,
                    &mut spans,
                );
            }
            if let Some(ref name) = self.resolved_namespace_name {
                spans.push(Span::styled(format!(" \u{2192} {name}"), theme.muted));
            }
        } else if self.namespace_input.is_empty() {
            spans.push(Span::styled(
                format!("({})", self.current_namespace_name),
                theme.muted,
            ));
        } else {
            spans.push(Span::raw(self.namespace_input.clone()));
            if let Some(ref name) = self.resolved_namespace_name {
                spans.push(Span::styled(format!(" \u{2192} {name}"), theme.muted));
            }
        }

        Line::from(spans).render(area, buf);
    }

    fn render_parent_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let label_style = field_label_style(self.focused == MoveFormField::Parent, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Parent:     ", label_style)];

        if self.focused == MoveFormField::Parent {
            if self.parent_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                spans.push(Span::styled(" (doc ID)", theme.muted));
            } else {
                render_cursor_spans(&self.parent_input, self.parent_cursor, theme, &mut spans);
            }
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(format!(" \u{2192} {title}"), theme.muted));
            }
        } else if self.parent_input.is_empty() {
            spans.push(Span::styled("(none)", theme.muted));
        } else {
            spans.push(Span::raw(self.parent_input.clone()));
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(format!(" \u{2192} {title}"), theme.muted));
            }
        }

        Line::from(spans).render(area, buf);
    }

    fn render_buttons(&self, _theme: &Theme, area: Rect, buf: &mut Buffer) {
        let move_focused = self.focused == MoveFormField::Move;
        let cancel_focused = self.focused == MoveFormField::Cancel;

        let move_style = if !self.can_submit() {
            Style::default().fg(Color::DarkGray)
        } else if move_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let cancel_style = if cancel_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };

        let hint = Line::from(vec![
            Span::raw("  "),
            Span::styled(" Move ", move_style),
            Span::raw("  "),
            Span::styled(" Cancel ", cancel_style),
        ]);
        hint.alignment(Alignment::Left).render(area, buf);
    }
}

/// Style for a field label: accent+bold when focused, default otherwise.
fn field_label_style(focused: bool, theme: &Theme) -> Style {
    if focused {
        theme.accent.add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// Append cursor-highlighted spans for a text field to the span list.
fn render_cursor_spans<'a>(text: &str, cursor: usize, theme: &Theme, spans: &mut Vec<Span<'a>>) {
    let (before, after) = text.split_at(cursor);
    spans.push(Span::raw(before.to_string()));
    spans.push(Span::styled(
        if after.is_empty() {
            "\u{2588}".to_string()
        } else {
            after.chars().next().unwrap().to_string()
        },
        theme.selected,
    ));
    if after.len() > 1 {
        spans.push(Span::raw(
            after[after.chars().next().unwrap().len_utf8()..].to_string(),
        ));
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
