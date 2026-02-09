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
use crate::{Error, Result, utils};

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
    let full_id = utils::resolve_task_id(&client, task_id).await?;

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

    // Fetch the task before updating to show changes
    let old_task = client.get_task(&full_id).await?;

    let req = UpdateTaskRequest {
        title: title.clone(),
        body: body.clone(),
        priority: priority.clone(),
        size: size.clone(),
        deadline: deadline.clone(),
    };

    let task = client.update_task(&full_id, &req).await?;

    println!("{}", "✓ Task updated successfully!".green().bold());
    println!("  ID: {}", task.id.cyan());

    // Show what changed with highlighting
    if let Some(new_title) = title {
        if old_task.title != new_title {
            println!(
                "  {} {} → {}",
                "Title:".bold(),
                old_task.title.dimmed().strikethrough(),
                new_title.green()
            );
        }
    }

    if let Some(new_priority) = priority {
        if old_task.priority != new_priority {
            println!(
                "  {} {} → {}",
                "Priority:".bold(),
                old_task.priority.dimmed().strikethrough(),
                new_priority.green()
            );
        }
    }

    if let Some(new_size) = size {
        if old_task.size != new_size {
            println!(
                "  {} {} → {}",
                "Size:".bold(),
                old_task.size.dimmed().strikethrough(),
                new_size.green()
            );
        }
    }

    if deadline.is_some() {
        let old_deadline_str = old_task
            .deadline
            .as_deref()
            .unwrap_or("none");
        let new_deadline_str = task
            .deadline
            .as_deref()
            .unwrap_or("none");

        if old_deadline_str != new_deadline_str {
            println!(
                "  {} {} → {}",
                "Deadline:".bold(),
                old_deadline_str.dimmed().strikethrough(),
                new_deadline_str.green()
            );
        }
    }

    if let Some(_new_body) = body {
        if old_task.body != task.body {
            println!("  {} {}", "Body:".bold(), "updated".green());
        }
    }

    println!("\nView with: {}", format!("gtr show {}", task.id).dimmed());

    Ok(())
}
