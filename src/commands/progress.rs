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

//! Progress command implementation.

use chrono::Utc;
use colored::Colorize;
use dialoguer::Select;

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::Task;
use crate::{Result, utils};

/// Set task progress (local-first with optional sync).
///
/// When no task_id is provided, smart resolution picks from "doing" tasks
/// or falls back to all pending tasks.
pub async fn run(config: &Config, value: u8, task_id: Option<String>, no_sync: bool) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let full_id = if let Some(ref id) = task_id {
        utils::resolve_task_id(&client, id).await?
    } else {
        resolve_doing_task(&client, &ctx).await?
    };

    let mut task = ctx.load_task(&client, &full_id).await?;

    let old_progress = task.progress;

    task.progress = Some(value);
    task.modified = Utc::now().to_rfc3339();
    task.version += 1;

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!("{}", "✓ Progress updated locally!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);

    let old_str = old_progress
        .map(|p| format!("{}%", p))
        .unwrap_or_else(|| "none".to_string());
    println!(
        "  Progress: {} → {}",
        old_str.dimmed().strikethrough(),
        format!("{}%", value).green()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync".yellow());
        }
    }

    Ok(())
}

/// Resolve a task when no task_id is provided.
///
/// 1. Load all pending tasks from cache/storage
/// 2. Filter to "doing" tasks
/// 3. Exactly 1 → use it
/// 4. Multiple doing → dialoguer::Select picker
/// 5. 0 doing → picker with ALL pending tasks
async fn resolve_doing_task(client: &Client, ctx: &LocalContext) -> Result<String> {
    // Get all projects
    let projects = client.list_projects().await?;

    // Load all pending tasks
    let mut pending_tasks: Vec<Task> = Vec::new();
    for project in &projects {
        let summaries = ctx.cache.list_tasks(&project.id)?;
        for summary in summaries {
            if summary.done.is_none()
                && summary.deleted.is_none()
                && let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id)
                && task.is_pending()
            {
                pending_tasks.push(task);
            }
        }
    }

    if pending_tasks.is_empty() {
        return Err(crate::Error::UserFacing(
            "No pending tasks found".to_string(),
        ));
    }

    // Filter to "doing" tasks
    let doing_tasks: Vec<&Task> = pending_tasks
        .iter()
        .filter(|t| t.current_work_state.as_deref() == Some("doing"))
        .collect();

    let selected = match doing_tasks.len() {
        1 => return Ok(doing_tasks[0].id.clone()),
        0 => {
            // No doing tasks — pick from all pending
            pick_task(&pending_tasks)?
        }
        _ => {
            // Multiple doing tasks — pick from doing
            let doing_owned: Vec<Task> = doing_tasks.into_iter().cloned().collect();
            pick_task(&doing_owned)?
        }
    };

    Ok(selected)
}

/// Interactive task picker using dialoguer::Select.
fn pick_task(tasks: &[Task]) -> Result<String> {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            let progress_str = t.progress.map(|p| format!(" ({}%)", p)).unwrap_or_default();
            format!("{} {}{}", t.id[..8].cyan(), t.title, progress_str.dimmed())
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select task")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    Ok(tasks[selection].id.clone())
}
