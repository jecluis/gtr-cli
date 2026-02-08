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

//! Update command implementation.

use crate::Result;
use crate::config::Config;

/// Update a task.
pub async fn run(
    _config: &Config,
    task_id: &str,
    title: Option<String>,
    body: Option<String>,
    priority: Option<String>,
    size: Option<String>,
) -> Result<()> {
    println!("Update task {} - to be implemented", task_id);
    println!("  Title: {:?}", title);
    println!("  Body: {:?}", body);
    println!("  Priority: {:?}", priority);
    println!("  Size: {:?}", size);
    Ok(())
}
