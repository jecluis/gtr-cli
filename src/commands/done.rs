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

use tracing::warn;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, TaskStatus};
use crate::utils;

/// Mark one or more tasks as done (local-first with optional sync).
pub async fn run(config: &Config, mut task_ids: Vec<String>, no_sync: bool) -> Result<()> {
    // If no task IDs provided, show picker
    if task_ids.is_empty() {
        let client = Client::new(config)?;
        let ctx = LocalContext::new(config, !no_sync)?;
        let selected_id =
            utils::pick_task(&client, &ctx, "Select task to mark as done", true).await?;
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

    // Cascade completion to all descendants
    let descendants = ctx.cache.get_all_descendants(&full_id)?;
    let descendant_count = descendants.len();
    for desc_id in descendants {
        match ctx.storage.load_task(&task.project_id, &desc_id) {
            Ok(mut desc_task) => {
                if desc_task.done.is_some() {
                    continue; // already done
                }
                desc_task.done = Some(now.to_rfc3339());
                desc_task.modified = now.to_rfc3339();
                desc_task.version += 1;
                desc_task.current_work_state = None;
                let old_prog = desc_task.progress;
                desc_task.progress = Some(100);
                desc_task.log.push(LogEntry {
                    timestamp: now,
                    entry_type: LogEntryType::StatusChanged {
                        status: TaskStatus::Done,
                    },
                    source: crate::models::LogSource::User,
                });
                desc_task.log.push(LogEntry {
                    timestamp: now,
                    entry_type: LogEntryType::ProgressChanged {
                        from: old_prog,
                        to: Some(100),
                    },
                    source: crate::models::LogSource::User,
                });
                ctx.storage.update_task(&desc_task.project_id, &desc_task)?;
                ctx.cache.upsert_task(&desc_task, true)?;
            }
            Err(e) => {
                warn!(task_id = %desc_id, error = %e, "failed to cascade done to descendant");
            }
        }
    }

    if descendant_count > 0 {
        println!(
            "  {}",
            format!("+ {} subtask(s) also marked done", descendant_count)
                .green()
                .bold()
        );
    }

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
