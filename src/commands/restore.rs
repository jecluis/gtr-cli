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

//! Restore command implementation.

use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::{Result, utils};

/// Restore a deleted task.
pub async fn run(config: &Config, task_id: &str) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let task = client.restore_task(&full_id).await?;

    println!("{}", "✓ Task restored from deleted!".green().bold());
    println!("  ID:    {}", task.id.to_string().cyan());
    println!("  Title: {}", task.title);

    Ok(())
}
