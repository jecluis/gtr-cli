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

//! Done command implementation.

use chrono::Utc;
use colored::Colorize;
use dialoguer::Select;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, Task, TaskStatus};
use crate::utils;

/// Mark one or more tasks as done (local-first with optional sync).
pub async fn run(config: &Config, mut task_ids: Vec<String>, no_sync: bool) -> Result<()> {
    // If no task IDs provided, show picker
    if task_ids.is_empty() {
        let client = Client::new(config)?;
        let ctx = LocalContext::new(config, !no_sync)?;
        let selected_id = pick_pending_task(&client, &ctx).await?;
        task_ids.push(selected_id);
    }

    let mut success_count = 0;
    let mut failures = Vec::new();

    for task_id in task_ids {
        match mark_task_done(config, &task_id, no_sync).await {
            Ok(title) => {
                success_count += 1;
                println!("{}", "✓ Task marked as done locally!".green().bold());
                println!("  ID:    {}", task_id.cyan());
                println!("  Title: {}", title);
            }
            Err(e) => {
                failures.push((task_id, e));
            }
        }
    }

    // Print summary
    if success_count > 0 {
        println!(
            "\n{}",
            format!("✓ Marked {} task(s) as done", success_count)
                .green()
                .bold()
        );
    }

    if !failures.is_empty() {
        eprintln!("\n{}", "✗ Failures:".red().bold());
        for (id, err) in failures {
            eprintln!("  {} - {}", id.red(), err);
        }
        return Err(crate::Error::UserFacing(
            "Some tasks failed to be marked as done".to_string(),
        ));
    }

    Ok(())
}

/// Mark a single task as done.
async fn mark_task_done(config: &Config, task_id: &str, no_sync: bool) -> Result<String> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Load task
    let mut task = ctx.load_task(&client, &full_id).await?;
    let title = task.title.clone();

    // Mark as done
    let now = Utc::now();
    task.done = Some(now.to_rfc3339());
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Clear work state when marking as done
    task.current_work_state = None;

    // Add log entry for status change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::StatusChanged {
            status: TaskStatus::Done,
        },
        source: crate::models::LogSource::User,
    });

    // Auto-set progress to 100%
    let old_progress = task.progress;
    task.progress = Some(100);
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::ProgressChanged {
            from: old_progress,
            to: Some(100),
        },
        source: crate::models::LogSource::User,
    });

    // Save locally
    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    // Sync
    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync".yellow());
        }
    }

    Ok(title)
}

/// Resolve a task when no task_id is provided.
///
/// Load all pending tasks from cache/storage and show picker.
async fn pick_pending_task(client: &Client, ctx: &LocalContext) -> Result<String> {
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

    pick_task(&pending_tasks)
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
        .with_prompt("Select task to mark as done")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    Ok(tasks[selection].id.clone())
}
