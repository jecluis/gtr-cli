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

//! Undone command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;

/// Unmark a task as done (restore to pending).
pub async fn run(config: &Config, task_id: &str) -> Result<()> {
    let client = Client::new(config)?;

    let task = client.mark_undone(task_id).await?;

    println!("{}", "✓ Task restored to pending!".green().bold());
    println!("  ID:    {}", task.id.to_string().cyan());
    println!("  Title: {}", task.title);

    Ok(())
}
