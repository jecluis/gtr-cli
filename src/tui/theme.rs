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

//! TUI colour theme derived from the CLI's existing colour conventions.
//!
//! Provides semantic style names so widgets don't hard-code colours.
//! Mirrors the CLI palette: cyan for identifiers, green for success,
//! red for danger, yellow for warnings, dim for secondary text.

use ratatui::style::{Color, Modifier, Style};

/// Semantic colour theme for the TUI.
pub struct Theme {
    /// Primary accent (identifiers, keys, links, project names).
    pub accent: Style,
    /// Secondary/muted text.
    pub muted: Style,
    /// Emphasis (titles, headings).
    pub emphasis: Style,
    /// Success / new values.
    pub success: Style,
    /// Danger / critical / deletions.
    pub danger: Style,
    /// Warning / deadlines / attention.
    pub warning: Style,
    /// Old/replaced values (strikethrough + dim).
    pub stale: Style,
    /// Status bar background.
    pub status_bar: Style,
    /// Status bar key hints.
    pub status_key: Style,
    /// Status bar descriptions.
    pub status_desc: Style,
    /// Focused/selected item in lists.
    pub selected: Style,
    /// Borders on focused panels.
    pub border_focused: Style,
    /// Borders on unfocused panels.
    pub border_unfocused: Style,
    /// Alternating row background tint (subtle, for visual grouping).
    pub row_alt_bg: Color,
    /// Divider line between sections (e.g. doing vs backlog).
    pub divider: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Style::new().fg(Color::Cyan),
            muted: Style::new().add_modifier(Modifier::DIM),
            emphasis: Style::new().add_modifier(Modifier::BOLD),
            success: Style::new().fg(Color::Green),
            danger: Style::new().fg(Color::Red),
            warning: Style::new().fg(Color::Yellow),
            stale: Style::new()
                .add_modifier(Modifier::DIM)
                .add_modifier(Modifier::CROSSED_OUT),
            status_bar: Style::new().fg(Color::White).bg(Color::DarkGray),
            status_key: Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            status_desc: Style::new().add_modifier(Modifier::DIM),
            selected: Style::new().bg(Color::DarkGray).fg(Color::White),
            border_focused: Style::new().fg(Color::Cyan),
            border_unfocused: Style::new().fg(Color::DarkGray),
            row_alt_bg: Color::Indexed(235),
            divider: Style::new().fg(Color::Gray),
        }
    }
}
