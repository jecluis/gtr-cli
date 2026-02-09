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

use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::models::UpdateTaskRequest;
use crate::{Error, Result};

/// Update a task.
pub async fn run(
    config: &Config,
    task_id: &str,
    title: Option<String>,
    body: Option<String>,
    priority: Option<String>,
    size: Option<String>,
    deadline: Option<String>,
) -> Result<()> {
    let client = Client::new(config)?;

    // Check if at least one field is provided
    if title.is_none()
        && body.is_none()
        && priority.is_none()
        && size.is_none()
        && deadline.is_none()
    {
        return Err(Error::InvalidInput(
            "at least one field must be provided to update".to_string(),
        ));
    }

    let req = UpdateTaskRequest {
        title,
        body,
        priority,
        size,
        deadline,
    };

    let task = client.update_task(task_id, &req).await?;

    println!("{}", "✓ Task updated successfully!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Priority: {}", task.priority);
    println!("  Size:     {}", task.size);
    println!("\nView with: {}", format!("gtr show {}", task.id).dimmed());

    Ok(())
}
