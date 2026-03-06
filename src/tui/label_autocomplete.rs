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

//! Inline autocomplete overlay for label fields.
//!
//! Shows matching existing labels as the user types, supporting
//! keyboard navigation with Up/Down and selection with Enter.
//! Labels are annotated with their source: `S` for labels defined
//! on the current project/namespace, `P` for inherited labels.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};

use crate::display::LABEL_PALETTE_LEN;
use crate::tui::theme::LABEL_PALETTE;

const MAX_SUGGESTIONS: usize = 4;

/// Autocomplete state for a label input field.
pub struct LabelAutocomplete {
    /// Available labels with source: `(label, is_own)`.
    /// `is_own = true` means defined on the current project/namespace,
    /// `false` means inherited from a parent.
    available: Vec<(String, bool)>,
    /// Filtered suggestions: `(label, is_own)`.
    suggestions: Vec<(String, bool)>,
    selected: Option<usize>,
}

impl LabelAutocomplete {
    /// Create a new autocomplete with the given set of available labels.
    ///
    /// Each entry is `(label_name, is_own)` where `is_own` indicates
    /// whether the label is defined on the current entity or inherited.
    pub fn new(available: Vec<(String, bool)>) -> Self {
        Self {
            available,
            suggestions: Vec::new(),
            selected: None,
        }
    }

    /// Re-filter suggestions based on the current query text.
    ///
    /// Labels already added to the form are excluded. Prefix matches
    /// sort before substring-only matches. Empty query clears
    /// suggestions (use [`show_all`] to browse without typing).
    pub fn update(&mut self, query: &str, current_labels: &[String]) {
        self.selected = None;

        if query.is_empty() {
            self.suggestions.clear();
            return;
        }

        self.filter(query, current_labels);
    }

    /// Show all available labels (excluding already-added ones).
    ///
    /// Used when the user presses Down or Space on an empty label
    /// input to browse the full list.
    pub fn show_all(&mut self, current_labels: &[String]) {
        self.selected = None;
        self.suggestions = self
            .available
            .iter()
            .filter(|(label, _)| !current_labels.contains(label))
            .take(MAX_SUGGESTIONS)
            .cloned()
            .collect();
    }

    /// Move selection to the next suggestion (wrapping).
    pub fn select_next(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => (i + 1) % self.suggestions.len(),
            None => 0,
        });
    }

    /// Move selection to the previous suggestion (wrapping).
    pub fn select_prev(&mut self) {
        if self.suggestions.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(0) => self.suggestions.len() - 1,
            Some(i) => i - 1,
            None => self.suggestions.len() - 1,
        });
    }

    /// The currently selected label text, if any.
    pub fn selected_label(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.suggestions.get(i))
            .map(|(s, _)| s.as_str())
    }

    /// Whether there are any suggestions to show.
    pub fn has_suggestions(&self) -> bool {
        !self.suggestions.is_empty()
    }

    /// Render the suggestion overlay into the given area.
    ///
    /// Each suggestion is rendered with a source prefix (`S`/`P`) and
    /// its palette colour. The selected suggestion gets a DarkGray
    /// background.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.suggestions.is_empty() {
            return;
        }

        let rows = self.suggestions.len().min(area.height as usize);
        let overlay = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: rows as u16,
        };

        Clear.render(overlay, buf);

        for (i, (label, is_own)) in self.suggestions.iter().take(rows).enumerate() {
            let color = LABEL_PALETTE[i % LABEL_PALETTE_LEN];
            let is_selected = self.selected == Some(i);

            let label_style = if is_selected {
                Style::default().fg(color).bg(Color::DarkGray)
            } else {
                Style::default().fg(color)
            };

            let source_tag = if *is_own { "S" } else { "P" };
            let source_style = if is_selected {
                Style::default().fg(Color::Gray).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };

            let row_area = Rect {
                x: overlay.x,
                y: overlay.y + i as u16,
                width: overlay.width,
                height: 1,
            };

            // Fill background for selected row
            if is_selected {
                for x in row_area.x..row_area.x + row_area.width {
                    buf[(x, row_area.y)].set_style(Style::default().bg(Color::DarkGray));
                }
            }

            Line::from(vec![
                Span::raw("    "),
                Span::styled(source_tag, source_style),
                Span::raw(" "),
                Span::styled(label.clone(), label_style),
            ])
            .render(row_area, buf);
        }
    }

    fn filter(&mut self, query: &str, current_labels: &[String]) {
        let query_lower = query.to_lowercase();

        let mut prefix_matches = Vec::new();
        let mut substring_matches = Vec::new();

        for (label, is_own) in &self.available {
            if current_labels.contains(label) {
                continue;
            }
            let label_lower = label.to_lowercase();
            if label_lower.starts_with(&query_lower) {
                prefix_matches.push((label.clone(), *is_own));
            } else if label_lower.contains(&query_lower) {
                substring_matches.push((label.clone(), *is_own));
            }
        }

        prefix_matches.extend(substring_matches);
        prefix_matches.truncate(MAX_SUGGESTIONS);
        self.suggestions = prefix_matches;
    }
}
