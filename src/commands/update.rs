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

use chrono::Utc;
use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::utils;
use crate::{Error, Result};

/// Update a task (local-first with optional sync).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &Config,
    task_id: &str,
    title: Option<String>,
    edit_body: bool,
    priority: Option<String>,
    size: Option<String>,
    deadline: Option<String>,
    no_sync: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    // Check if at least one field is provided
    if title.is_none() && !edit_body && priority.is_none() && size.is_none() && deadline.is_none() {
        return Err(Error::UserFacing(
            "No updates specified. Provide at least one field to update (--title, --body, --priority, --size, or --deadline)".to_string(),
        ));
    }

    // Initialize local context
    let ctx = LocalContext::new(config, !no_sync)?;

    // Load task from local storage (or fetch from server if not cached)
    let mut task = match ctx.storage.load_task("", &full_id) {
        Ok(t) => t,
        Err(_) => {
            // Not in local storage, fetch from server
            let fetched = client.get_task(&full_id).await?;
            ctx.storage.create_task(&fetched.project_id, &fetched)?;
            ctx.cache.upsert_task(&fetched, false)?;
            fetched
        }
    };

    let old_task = task.clone();

    // Edit body if requested (includes title as H1 header)
    if edit_body {
        match crate::editor::edit_task_body(config, &task.title, &task.body) {
            Ok((new_title, new_body)) => {
                task.body = new_body;
                // Update title if it changed in editor
                if let Some(title_from_editor) = new_title {
                    task.title = title_from_editor;
                }
            }
            Err(crate::Error::InvalidInput(ref msg)) if msg == "Operation cancelled" => {
                println!("{}", "✗ Operation cancelled".yellow());
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }

    // Apply updates
    if let Some(ref new_title) = title {
        task.title = new_title.clone();
    }
    if let Some(ref new_priority) = priority {
        task.priority = new_priority.clone();
    }
    if let Some(ref new_size) = size {
        task.size = new_size.clone();
    }
    if let Some(ref new_deadline) = deadline {
        task.deadline = if new_deadline == "none" {
            None
        } else {
            // Validate deadline format
            Some(crate::utils::validate_deadline(new_deadline)?)
        };
    }

    // Update metadata
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    // Save locally
    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, !no_sync)?;

    println!("{}", "✓ Task updated locally!".green().bold());
    println!("  ID: {}", task.id.cyan());

    // Show what changed with highlighting
    if let Some(new_title) = title {
        // Title changed via --title flag
        if old_task.title != new_title {
            println!(
                "  {} {} → {}",
                "Title:".bold(),
                old_task.title.dimmed().strikethrough(),
                new_title.green()
            );
        }
    } else if edit_body && old_task.title != task.title {
        // Title changed via editor
        println!(
            "  {} {} → {}",
            "Title:".bold(),
            old_task.title.dimmed().strikethrough(),
            task.title.green()
        );
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
        let old_deadline_str = old_task.deadline.as_deref().unwrap_or("none");
        let new_deadline_str = task.deadline.as_deref().unwrap_or("none");

        if old_deadline_str != new_deadline_str {
            println!(
                "  {} {} → {}",
                "Deadline:".bold(),
                old_deadline_str.dimmed().strikethrough(),
                new_deadline_str.green()
            );
        }
    }

    if edit_body && old_task.body != task.body {
        println!("  {} {}", "Body:".bold(), "updated".green());
    }

    // Attempt sync if enabled
    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync (server unreachable)".yellow());
        }
    }

    println!("\nView with: {}", format!("gtr show {}", task.id).dimmed());

    Ok(())
}
