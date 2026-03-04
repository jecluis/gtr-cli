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

//! Reusable inline editor — title + body editing for documents, tasks,
//! and any future entity that needs a two-field text editor.

use rat_widget::textarea::{TextArea, TextAreaState, TextWrap};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, StatefulWidget, Widget};

use crate::tui::theme::Theme;

/// Border colour for the focused editor field (title or body text area).
/// Deliberately different from `theme.border_focused` (cyan) so the
/// editor's input borders stand out without clashing with the accent.
const FIELD_BORDER_FOCUSED: Style = Style::new().fg(Color::Yellow);

/// Which field currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFocus {
    Title,
    Body,
}

/// Reusable state for an inline title + body editor.
pub struct InlineEditorState {
    /// Single-line title input.
    title_input: String,
    title_cursor: usize,

    /// Multi-line body via rat-widget TextArea.
    pub body: TextAreaState,

    /// Which field has focus.
    pub focus: EditorFocus,

    /// Original values for dirty detection.
    pub original_title: String,
    original_body: String,
}

impl InlineEditorState {
    /// Create an editor pre-loaded with existing content.
    pub fn new(title: String, body: String) -> Self {
        let mut body_state = TextAreaState::new();
        body_state.set_text(&body);

        Self {
            title_input: title.clone(),
            title_cursor: title.len(),
            body: body_state,
            focus: EditorFocus::Body,
            original_title: title,
            original_body: body,
        }
    }

    /// Whether the content has been modified.
    pub fn is_dirty(&self) -> bool {
        self.title_input != self.original_title || self.body.text() != self.original_body
    }

    /// Current title text.
    pub fn title(&self) -> &str {
        &self.title_input
    }

    /// Current body text.
    pub fn body_text(&self) -> String {
        self.body.text()
    }

    /// Toggle focus between title and body.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            EditorFocus::Title => EditorFocus::Body,
            EditorFocus::Body => EditorFocus::Title,
        };
    }

    // -- Title input handling --

    /// Insert a character into the title at the cursor position.
    pub fn title_char_input(&mut self, c: char) {
        if c == '\n' || c == '\r' {
            return; // single-line
        }
        self.title_input.insert(self.title_cursor, c);
        self.title_cursor += c.len_utf8();
    }

    /// Delete the character before the cursor in the title.
    pub fn title_backspace(&mut self) {
        if self.title_cursor > 0 {
            let prev = self.title_input[..self.title_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.title_input.remove(prev);
            self.title_cursor = prev;
        }
    }

    /// Delete the character at the cursor in the title.
    pub fn title_delete(&mut self) {
        if self.title_cursor < self.title_input.len() {
            self.title_input.remove(self.title_cursor);
        }
    }

    /// Move title cursor left.
    pub fn title_cursor_left(&mut self) {
        if self.title_cursor > 0 {
            self.title_cursor = self.title_input[..self.title_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move title cursor right.
    pub fn title_cursor_right(&mut self) {
        if self.title_cursor < self.title_input.len() {
            let rest = &self.title_input[self.title_cursor..];
            if let Some(c) = rest.chars().next() {
                self.title_cursor += c.len_utf8();
            }
        }
    }

    /// Move title cursor to start.
    pub fn title_home(&mut self) {
        self.title_cursor = 0;
    }

    /// Move title cursor to end.
    pub fn title_end(&mut self) {
        self.title_cursor = self.title_input.len();
    }

    /// Render the editor filling the given area.
    ///
    /// `block_title` is the label shown on the outer border (e.g.
    /// `" edit document "` or `" edit task "`).
    pub fn render(
        &mut self,
        theme: &Theme,
        focused: bool,
        block_title: &str,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let border_style = if focused {
            theme.border_focused
        } else {
            theme.border_unfocused
        };

        let block = Block::bordered()
            .title(block_title)
            .border_style(border_style)
            .padding(Padding::new(1, 1, 1, 0));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 9 || inner.width < 10 {
            return;
        }

        let rows = Layout::vertical([
            Constraint::Length(1), // "Title:" label
            Constraint::Length(3), // title input (bordered)
            Constraint::Length(1), // gap
            Constraint::Length(1), // "Body:" label
            Constraint::Fill(1),   // body textarea
        ])
        .split(inner);

        // Title label — always bold, accent colour when focused.
        let title_label_style = if self.focus == EditorFocus::Title {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray).add_modifier(Modifier::BOLD)
        };
        Line::from(Span::styled("Title", title_label_style)).render(rows[0], buf);

        // Title input inside a bordered block.
        let title_focused = focused && self.focus == EditorFocus::Title;
        let title_block = Block::bordered()
            .border_style(if title_focused {
                FIELD_BORDER_FOCUSED
            } else {
                theme.border_unfocused
            })
            .padding(Padding::horizontal(1));
        let title_inner = title_block.inner(rows[1]);
        title_block.render(rows[1], buf);
        self.render_title_input(theme, title_inner, buf);

        // Body label — always bold, accent colour when focused.
        let body_label_style = if self.focus == EditorFocus::Body {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray).add_modifier(Modifier::BOLD)
        };
        Line::from(Span::styled("Body", body_label_style)).render(rows[3], buf);

        // Body textarea
        let body_focused = focused && self.focus == EditorFocus::Body;
        self.body.focus.set(body_focused);
        let body_block = Block::bordered()
            .border_style(if body_focused {
                FIELD_BORDER_FOCUSED
            } else {
                theme.border_unfocused
            })
            .padding(Padding::horizontal(1));

        let body_widget = TextArea::new()
            .block(body_block)
            .style(Style::default())
            .focus_style(Style::default())
            .cursor_style(Style::new().bg(Color::White).fg(Color::Black))
            .text_wrap(TextWrap::Word(5));

        StatefulWidget::render(body_widget, rows[4], buf, &mut self.body);
    }

    /// Render the title input with a block cursor indicator.
    fn render_title_input(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let show_cursor = self.focus == EditorFocus::Title;
        let mut spans = Vec::new();

        if !self.title_input.is_empty() {
            let (before, after) = self.title_input.split_at(self.title_cursor);
            spans.push(Span::raw(before.to_string()));
            if show_cursor {
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
                spans.push(Span::raw(after.to_string()));
            }
        } else if show_cursor {
            spans.push(Span::styled("\u{2588}", theme.selected));
        }

        Line::from(spans).render(area, buf);
    }
}
