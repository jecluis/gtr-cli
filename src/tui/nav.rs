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

//! Navigation types for cross-entity link following in detail views.

/// Target entity for a navigable link.
#[derive(Debug, Clone)]
pub enum NavTarget {
    Task { id: String },
    Document { id: String },
}

/// A navigable link within a detail view's content.
#[derive(Debug, Clone)]
pub struct NavLink {
    /// The entity this link points to.
    pub target: NavTarget,
    /// The line index in the built content where this link appears.
    pub line_index: usize,
}
