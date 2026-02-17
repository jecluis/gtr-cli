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

use chrono::Utc;
use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::hierarchy;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, TaskStatus};
use crate::utils;

/// Unmark a task as done (local-first with optional sync).
pub async fn run(
    config: &Config,
    task_id: &str,
    progress: Option<u8>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    let mut task = ctx.load_task(&client, &full_id).await?;

    let now = Utc::now();
    task.done = None;
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Add log entry for status change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::StatusChanged {
            status: TaskStatus::Restored,
        },
        source: crate::models::LogSource::User,
    });

    // Auto-set progress to 50% (or user-provided value)
    let old_progress = task.progress;
    task.progress = Some(progress.unwrap_or(50));
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::ProgressChanged {
            from: old_progress,
            to: task.progress,
        },
        source: crate::models::LogSource::User,
    });

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    // Update ancestor progress if this task has a parent
    if task.parent_id.is_some() {
        hierarchy::update_ancestor_progress(&ctx.cache, &ctx.storage, &task.project_id, &task.id)?;
    }

    println!(
        "{}",
        format!("{} Task restored to pending locally!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:    {}", task.id.cyan());
    println!("  Title: {}", task.display_title(&icons));

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}
