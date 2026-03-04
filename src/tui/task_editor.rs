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

//! Inline task editor — thin wrapper around [`InlineEditorState`]
//! that adds the task-specific `task_id` and `project_id` fields.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::tui::inline_editor::InlineEditorState;
use crate::tui::theme::Theme;

/// State for the inline task editor.
pub struct TaskEditorState {
    pub task_id: String,
    pub project_id: String,
    pub editor: InlineEditorState,
}

impl TaskEditorState {
    /// Create an editor pre-loaded with an existing task's content.
    pub fn new(task_id: String, project_id: String, title: String, body: String) -> Self {
        Self {
            task_id,
            project_id,
            editor: InlineEditorState::new(title, body),
        }
    }

    /// Render the editor filling the given area.
    pub fn render(&mut self, theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
        self.editor.render(theme, focused, " edit task ", area, buf);
    }
}
