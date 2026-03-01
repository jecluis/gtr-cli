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

//! Terminal user interface for gtr.
//!
//! Launched when `gtr` is invoked with no subcommand. Provides an
//! interactive, keyboard-driven interface for browsing and managing
//! tasks and documents.

mod app;
pub mod command_bar;
pub mod confirm;
pub mod create_form;
pub mod doc_detail;
pub mod doc_list;
pub mod keymap;
pub mod sidebar;
pub mod task_detail;
pub mod task_list;
pub mod theme;

/// Launch the TUI, taking over the terminal until the user quits.
pub fn run(config: crate::config::Config) -> crate::Result<()> {
    app::run(config)
}
