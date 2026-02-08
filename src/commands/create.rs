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

//! Create command implementation.

use crate::Result;
use crate::config::Config;

/// Create a new task.
pub async fn run(
    _config: &Config,
    project: &str,
    title: &str,
    body: Option<String>,
    priority: &str,
    size: &str,
) -> Result<()> {
    println!("Create task - to be implemented");
    println!("  Project: {}", project);
    println!("  Title: {}", title);
    println!("  Body: {:?}", body);
    println!("  Priority: {}", priority);
    println!("  Size: {}", size);
    Ok(())
}
