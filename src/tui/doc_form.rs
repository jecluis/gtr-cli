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

//! Centred overlay form for creating and editing documents.

use crossterm::event::{Event, KeyCode, KeyEvent};
use rat_widget::textarea::{TextArea, TextAreaState, TextWrap};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, StatefulWidget, Widget};

use crate::display::LABEL_PALETTE_LEN;
use crate::models::Document;
use crate::tui::label_autocomplete::LabelAutocomplete;
use crate::tui::theme::{LABEL_PALETTE, Theme};

/// Whether the form is creating a new document or editing an existing one.
pub enum DocFormMode {
    Create { namespace_id: String },
    Update { doc_id: String },
}

/// Which field has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DocFormField {
    Title,
    Labels,
    Parent,
    Save,
    Cancel,
}

/// Snapshot of original values for change detection in update mode.
struct OriginalValues {
    title: String,
    labels: Vec<String>,
    parent_id: Option<String>,
}

/// State for the document creation/edit form overlay.
pub struct DocFormState {
    mode: DocFormMode,
    namespace_name: String,
    original: Option<OriginalValues>,
    focused: DocFormField,
    // Fields
    title_ta: TextAreaState,
    labels: Vec<String>,
    label_input: String,
    label_cursor: usize,
    label_autocomplete: LabelAutocomplete,
    parent_input: String,
    parent_cursor: usize,
    resolved_parent_id: Option<String>,
    resolved_parent_title: Option<String>,
}

const FIELDS: &[DocFormField] = &[
    DocFormField::Title,
    DocFormField::Labels,
    DocFormField::Parent,
];

impl DocFormState {
    /// Create an empty form for a new document in the given namespace.
    pub fn new(
        namespace_id: String,
        namespace_name: String,
        available_labels: Vec<(String, bool)>,
    ) -> Self {
        Self {
            mode: DocFormMode::Create { namespace_id },
            namespace_name,
            original: None,
            focused: DocFormField::Title,
            title_ta: TextAreaState::new(),
            labels: Vec::new(),
            label_input: String::new(),
            label_cursor: 0,
            label_autocomplete: LabelAutocomplete::new(available_labels),
            parent_input: String::new(),
            parent_cursor: 0,
            resolved_parent_id: None,
            resolved_parent_title: None,
        }
    }

    /// Create a form pre-filled with an existing document's values.
    pub fn for_update(
        doc: Document,
        namespace_name: String,
        available_labels: Vec<(String, bool)>,
    ) -> Self {
        let parent_input = doc
            .parent_id
            .as_ref()
            .map(|id| id[..8.min(id.len())].to_string())
            .unwrap_or_default();

        let original = OriginalValues {
            title: doc.title.clone(),
            labels: doc.labels.clone(),
            parent_id: doc.parent_id.clone(),
        };

        let mut title_ta = TextAreaState::new();
        title_ta.set_text(&doc.title);

        Self {
            mode: DocFormMode::Update {
                doc_id: doc.id.clone(),
            },
            namespace_name,
            original: Some(original),
            focused: DocFormField::Title,
            title_ta,
            labels: doc.labels.clone(),
            label_input: String::new(),
            label_cursor: 0,
            label_autocomplete: LabelAutocomplete::new(available_labels),
            parent_input,
            parent_cursor: 0,
            resolved_parent_id: doc.parent_id.clone(),
            resolved_parent_title: None,
        }
    }

    pub fn mode(&self) -> &DocFormMode {
        &self.mode
    }

    pub fn title(&self) -> String {
        self.title_ta.text()
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.resolved_parent_id.as_deref()
    }

    pub fn doc_id(&self) -> Option<&str> {
        match &self.mode {
            DocFormMode::Update { doc_id } => Some(doc_id),
            DocFormMode::Create { .. } => None,
        }
    }

    /// Whether the title is non-empty (ready to submit).
    pub fn can_submit(&self) -> bool {
        !self.title_ta.text().trim().is_empty()
    }

    /// Whether the label input is currently non-empty.
    pub fn has_pending_label(&self) -> bool {
        !self.label_input.is_empty()
    }

    /// Whether any field has been modified from the original values.
    pub fn has_changes(&self) -> bool {
        self.changed_title().is_some()
            || self.changed_labels().is_some()
            || self.changed_parent_id().is_some()
    }

    /// Move focus to the next field.
    pub fn focus_next(&mut self) {
        match self.focused {
            DocFormField::Cancel => self.focused = FIELDS[0],
            DocFormField::Save => self.focused = DocFormField::Cancel,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos + 1 < FIELDS.len() {
                        self.focused = FIELDS[pos + 1];
                    } else {
                        self.focused = DocFormField::Save;
                    }
                }
            }
        }
        if self.focused != DocFormField::Labels {
            self.label_autocomplete.update("", &self.labels);
        }
    }

    /// Move focus to the previous field.
    pub fn focus_prev(&mut self) {
        match self.focused {
            DocFormField::Save => self.focused = *FIELDS.last().unwrap(),
            DocFormField::Cancel => self.focused = DocFormField::Save,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos > 0 {
                        self.focused = FIELDS[pos - 1];
                    } else {
                        self.focused = DocFormField::Cancel;
                    }
                }
            }
        }
        if self.focused != DocFormField::Labels {
            self.label_autocomplete.update("", &self.labels);
        }
    }

    /// Handle character input for text fields (excluding Title, which
    /// uses `handle_title_key`).
    pub fn char_input(&mut self, c: char) {
        match self.focused {
            DocFormField::Title => {
                self.forward_title_key(KeyEvent::new(
                    KeyCode::Char(c),
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            DocFormField::Labels => {
                if c == ',' {
                    self.commit_label();
                } else {
                    self.label_input.insert(self.label_cursor, c);
                    self.label_cursor += c.len_utf8();
                    self.label_autocomplete
                        .update(&self.label_input, &self.labels);
                }
            }
            DocFormField::Parent => {
                self.parent_input.insert(self.parent_cursor, c);
                self.parent_cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    /// Handle backspace.
    pub fn backspace(&mut self) {
        match self.focused {
            DocFormField::Title => {
                self.forward_title_key(KeyEvent::new(
                    KeyCode::Backspace,
                    crossterm::event::KeyModifiers::NONE,
                ));
            }
            DocFormField::Labels => {
                if self.label_cursor > 0 {
                    let prev = self.label_input[..self.label_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.label_input.remove(prev);
                    self.label_cursor = prev;
                    self.label_autocomplete
                        .update(&self.label_input, &self.labels);
                } else {
                    // Backspace on empty input removes last label
                    self.labels.pop();
                }
            }
            DocFormField::Parent => {
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

    /// Commit the current label input if valid.
    pub fn commit_label(&mut self) -> bool {
        let Ok(normalized) = crate::labels::normalize_label(&self.label_input) else {
            return false;
        };
        if self.labels.contains(&normalized) {
            self.label_input.clear();
            self.label_cursor = 0;
            self.label_autocomplete.update("", &self.labels);
            return false;
        }
        self.labels.push(normalized);
        self.label_input.clear();
        self.label_cursor = 0;
        self.label_autocomplete.update("", &self.labels);
        true
    }

    /// Move autocomplete selection to the next suggestion.
    pub fn autocomplete_select_next(&mut self) {
        self.label_autocomplete.select_next();
    }

    /// Move autocomplete selection to the previous suggestion.
    pub fn autocomplete_select_prev(&mut self) {
        self.label_autocomplete.select_prev();
    }

    /// Accept the currently selected autocomplete suggestion.
    pub fn accept_autocomplete(&mut self) -> bool {
        let Some(label) = self.label_autocomplete.selected_label().map(String::from) else {
            return false;
        };
        self.label_input = label;
        self.label_cursor = self.label_input.len();
        self.commit_label();
        true
    }

    /// Whether the autocomplete overlay should be shown.
    pub fn autocomplete_active(&self) -> bool {
        self.focused == DocFormField::Labels && self.label_autocomplete.has_suggestions()
    }

    /// Show all available labels (for browsing before typing).
    pub fn show_all_labels(&mut self) {
        self.label_autocomplete.show_all(&self.labels);
    }

    /// Set the resolved parent title for display feedback.
    pub fn set_resolved_parent(&mut self, title: Option<String>) {
        self.resolved_parent_title = title;
    }

    /// Store the resolved full parent UUID.
    pub fn set_parent_id(&mut self, full_id: Option<String>) {
        self.resolved_parent_id = full_id;
    }

    /// Current focused field.
    pub fn focused(&self) -> DocFormField {
        self.focused
    }

    /// Current parent input text (for resolution).
    pub fn parent_input(&self) -> &str {
        &self.parent_input
    }

    // -- Change detection methods (for update mode) --

    /// Returns `Some(title)` if title changed from original.
    pub fn changed_title(&self) -> Option<String> {
        let Some(orig) = &self.original else {
            return None;
        };
        let title_text = self.title_ta.text();
        (title_text != orig.title).then_some(title_text)
    }

    /// Returns `Some(labels)` if labels changed from original.
    pub fn changed_labels(&self) -> Option<Vec<String>> {
        let Some(orig) = &self.original else {
            return None;
        };
        (self.labels != orig.labels).then(|| self.labels.clone())
    }

    /// Returns `Some(Some(id))` or `Some(None)` if parent changed.
    pub fn changed_parent_id(&self) -> Option<Option<String>> {
        let Some(orig) = &self.original else {
            return None;
        };
        (self.resolved_parent_id != orig.parent_id).then(|| self.resolved_parent_id.clone())
    }

    /// Whether this form is in update mode.
    fn is_update(&self) -> bool {
        matches!(self.mode, DocFormMode::Update { .. })
    }

    /// Forward a key event to the title textarea (for cursor movement,
    /// character input, deletion). Blocks Enter to prevent newlines.
    pub fn handle_title_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {} // no newlines in title
            _ => self.forward_title_key(*key),
        }
    }

    /// Send a key event to the title textarea.
    fn forward_title_key(&mut self, key: KeyEvent) {
        let event = Event::Key(key);
        rat_widget::textarea::handle_events(&mut self.title_ta, true, &event);
    }

    pub fn render(&mut self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup_width = 72u16.min(area.width.saturating_sub(4));
        let popup_height = 14;
        let popup = centered_rect(popup_width, popup_height, area);

        Clear.render(popup, buf);

        let border_style = theme.accent;
        let title = if self.is_update() {
            format!(" Update Document ({}) ", self.namespace_name)
        } else {
            format!(" New Document ({}) ", self.namespace_name)
        };
        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([
            Constraint::Length(1), // [0] Title label
            Constraint::Length(3), // [1] Title input (bordered)
            Constraint::Length(1), // [2] padding
            Constraint::Length(1), // [3] Labels
            Constraint::Length(1), // [4] padding
            Constraint::Length(1), // [5] Parent
            Constraint::Length(1), // [6] padding
            Constraint::Length(1), // [7] padding
            Constraint::Length(1), // [8] padding
            Constraint::Length(1), // [9] buttons
        ])
        .split(inner);

        // Title field
        let title_style = field_label_style(self.focused == DocFormField::Title, theme);
        Line::from(vec![Span::raw("  "), Span::styled("Title: ", title_style)])
            .render(rows[0], buf);

        let title_focused = self.focused == DocFormField::Title;
        self.title_ta.focus.set(title_focused);
        let title_block = Block::bordered()
            .border_style(if title_focused {
                Style::new().fg(Color::Yellow)
            } else {
                theme.border_unfocused
            })
            .padding(Padding::horizontal(1));
        let title_widget = TextArea::new()
            .block(title_block)
            .style(Style::default())
            .focus_style(Style::default())
            .cursor_style(Style::new().bg(Color::White).fg(Color::Black))
            .text_wrap(TextWrap::Shift);
        title_widget.render(rows[1], buf, &mut self.title_ta);

        // Labels field
        self.render_labels_field(theme, rows[3], buf);

        // Parent field
        self.render_parent_field(theme, rows[5], buf);

        // Submit/Cancel buttons
        self.render_submit_buttons(theme, rows[9], buf);

        // Autocomplete overlay (rendered last so it covers fields below)
        if self.autocomplete_active() {
            let overlay = Rect {
                x: rows[4].x,
                y: rows[4].y,
                width: rows[4].width,
                height: rows
                    .get(7)
                    .map_or(0, |r| r.y + r.height)
                    .saturating_sub(rows[4].y),
            };
            self.label_autocomplete.render(overlay, buf);
        }
    }

    fn render_labels_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let label_style = field_label_style(self.focused == DocFormField::Labels, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Labels:  ", label_style)];

        for (i, label) in self.labels.iter().enumerate() {
            let color = LABEL_PALETTE[i % LABEL_PALETTE_LEN];
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(label.clone(), Style::default().fg(color)));
        }

        if self.focused == DocFormField::Labels {
            if !self.labels.is_empty() {
                spans.push(Span::raw(" "));
            }
            if self.label_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                if self.labels.is_empty() {
                    spans.push(Span::styled(
                        " (type to add)",
                        Style::default().fg(Color::Gray),
                    ));
                }
            } else {
                render_cursor_spans(&self.label_input, self.label_cursor, theme, &mut spans);
            }
        } else if self.labels.is_empty() {
            spans.push(Span::styled("(none)", Style::default().fg(Color::Gray)));
        }

        Line::from(spans).render(area, buf);
    }

    fn render_parent_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let label_style = field_label_style(self.focused == DocFormField::Parent, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Parent:  ", label_style)];

        if self.focused == DocFormField::Parent {
            if self.parent_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                spans.push(Span::styled(" (doc ID)", Style::default().fg(Color::Gray)));
            } else {
                render_cursor_spans(&self.parent_input, self.parent_cursor, theme, &mut spans);
            }
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(
                    format!(" \u{2192} {title}"),
                    Style::default().fg(Color::Gray),
                ));
            }
        } else if self.parent_input.is_empty() {
            spans.push(Span::styled("(none)", Style::default().fg(Color::Gray)));
        } else {
            spans.push(Span::raw(self.parent_input.clone()));
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(
                    format!(" \u{2192} {title}"),
                    Style::default().fg(Color::Gray),
                ));
            }
        }

        Line::from(spans).render(area, buf);
    }

    fn render_submit_buttons(&self, _theme: &Theme, area: Rect, buf: &mut Buffer) {
        let action_text = if self.is_update() {
            " Save "
        } else {
            " Create "
        };

        let save_focused = self.focused == DocFormField::Save;
        let cancel_focused = self.focused == DocFormField::Cancel;

        let save_style = if !self.can_submit() {
            Style::default().fg(Color::DarkGray)
        } else if save_focused {
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
            Span::styled(action_text, save_style),
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
