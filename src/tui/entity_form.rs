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

//! Centred overlay form for creating projects and namespaces.

use crossterm::event::{Event, KeyCode, KeyEvent};
use rat_widget::textarea::{TextArea, TextAreaState, TextWrap};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, StatefulWidget, Widget};

use crate::tui::theme::Theme;

/// Whether we are creating a project or a namespace.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Project,
    Namespace,
}

/// Which field has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EntityFormField {
    Name,
    Description,
    Save,
    Cancel,
}

const FIELDS: &[EntityFormField] = &[EntityFormField::Name, EntityFormField::Description];

/// State for the entity creation form overlay.
pub struct EntityFormState {
    kind: EntityKind,
    parent_id: Option<String>,
    parent_name: Option<String>,
    name_ta: TextAreaState,
    desc_ta: TextAreaState,
    focused: EntityFormField,
}

impl EntityFormState {
    /// Create an empty form for a new project or namespace.
    pub fn new(kind: EntityKind, parent_id: Option<String>, parent_name: Option<String>) -> Self {
        Self {
            kind,
            parent_id,
            parent_name,
            name_ta: TextAreaState::new(),
            desc_ta: TextAreaState::new(),
            focused: EntityFormField::Name,
        }
    }

    pub fn kind(&self) -> EntityKind {
        self.kind
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// Current name text.
    pub fn name(&self) -> String {
        self.name_ta.text()
    }

    /// Current description text.
    pub fn description(&self) -> String {
        self.desc_ta.text()
    }

    /// Whether the name is non-empty (ready to submit).
    pub fn can_submit(&self) -> bool {
        !self.name_ta.text().trim().is_empty()
    }

    /// Current focused field.
    pub fn focused(&self) -> EntityFormField {
        self.focused
    }

    /// Pre-fill the name field (used by command-bar `:new project <name>`).
    pub fn set_name(&mut self, name: &str) {
        self.name_ta.set_text(name);
    }

    /// Move focus to the next field.
    pub fn focus_next(&mut self) {
        match self.focused {
            EntityFormField::Cancel => self.focused = FIELDS[0],
            EntityFormField::Save => self.focused = EntityFormField::Cancel,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos + 1 < FIELDS.len() {
                        self.focused = FIELDS[pos + 1];
                    } else {
                        self.focused = EntityFormField::Save;
                    }
                }
            }
        }
    }

    /// Move focus to the previous field.
    pub fn focus_prev(&mut self) {
        match self.focused {
            EntityFormField::Save => self.focused = *FIELDS.last().unwrap(),
            EntityFormField::Cancel => self.focused = EntityFormField::Save,
            _ => {
                if let Some(pos) = FIELDS.iter().position(|f| *f == self.focused) {
                    if pos > 0 {
                        self.focused = FIELDS[pos - 1];
                    } else {
                        self.focused = EntityFormField::Cancel;
                    }
                }
            }
        }
    }

    /// Forward a key event to the name textarea.
    /// Blocks Enter to prevent newlines in the name field.
    pub fn handle_name_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {} // no newlines in name
            _ => {
                let event = Event::Key(*key);
                rat_widget::textarea::handle_events(&mut self.name_ta, true, &event);
            }
        }
    }

    /// Forward a key event to the description textarea.
    /// Enter is allowed for multi-line input.
    pub fn handle_desc_key(&mut self, key: &KeyEvent) {
        let event = Event::Key(*key);
        rat_widget::textarea::handle_events(&mut self.desc_ta, true, &event);
    }

    pub fn render(&mut self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup_width = 72u16.min(area.width.saturating_sub(4));
        let popup_height = 14;
        let popup = centered_rect(popup_width, popup_height, area);

        Clear.render(popup, buf);

        let border_style = theme.accent;
        let kind_label = match self.kind {
            EntityKind::Project => "Project",
            EntityKind::Namespace => "Namespace",
        };
        let title = if let Some(ref pname) = self.parent_name {
            format!(" New {kind_label} ({pname}) ")
        } else {
            format!(" New {kind_label} ")
        };
        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(popup);
        block.render(popup, buf);

        let rows = Layout::vertical([
            Constraint::Length(1), // [0] Name label
            Constraint::Length(3), // [1] Name input (bordered)
            Constraint::Length(1), // [2] gap
            Constraint::Length(1), // [3] Description label
            Constraint::Length(4), // [4] Description input (bordered, 4 rows)
            Constraint::Length(1), // [5] gap
            Constraint::Length(1), // [6] buttons
        ])
        .split(inner);

        // Name field
        let name_style = field_label_style(self.focused == EntityFormField::Name, theme);
        Line::from(vec![Span::raw("  "), Span::styled("Name: ", name_style)]).render(rows[0], buf);

        let name_focused = self.focused == EntityFormField::Name;
        self.name_ta.focus.set(name_focused);
        let name_block = Block::bordered()
            .border_style(if name_focused {
                Style::new().fg(Color::Yellow)
            } else {
                theme.border_unfocused
            })
            .padding(Padding::horizontal(1));
        let name_widget = TextArea::new()
            .block(name_block)
            .style(Style::default())
            .focus_style(Style::default())
            .cursor_style(Style::new().bg(Color::White).fg(Color::Black))
            .text_wrap(TextWrap::Shift);
        name_widget.render(rows[1], buf, &mut self.name_ta);

        // Description field
        let desc_style = field_label_style(self.focused == EntityFormField::Description, theme);
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Description: ", desc_style),
        ])
        .render(rows[3], buf);

        let desc_focused = self.focused == EntityFormField::Description;
        self.desc_ta.focus.set(desc_focused);
        let desc_block = Block::bordered()
            .border_style(if desc_focused {
                Style::new().fg(Color::Yellow)
            } else {
                theme.border_unfocused
            })
            .padding(Padding::horizontal(1));
        let desc_widget = TextArea::new()
            .block(desc_block)
            .style(Style::default())
            .focus_style(Style::default())
            .cursor_style(Style::new().bg(Color::White).fg(Color::Black))
            .text_wrap(TextWrap::Word(5));
        desc_widget.render(rows[4], buf, &mut self.desc_ta);

        // Submit/Cancel buttons
        self.render_buttons(theme, rows[6], buf);
    }

    fn render_buttons(&self, _theme: &Theme, area: Rect, buf: &mut Buffer) {
        let save_focused = self.focused == EntityFormField::Save;
        let cancel_focused = self.focused == EntityFormField::Cancel;

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
            Span::styled(" Create ", save_style),
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
