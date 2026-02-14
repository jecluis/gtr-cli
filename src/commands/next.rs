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

//! Next command implementation - suggests tasks to work on based on urgency.

use chrono::Utc;
use colored::Colorize;
use dialoguer::Select;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType, Task, WorkState};

/// Suggest next tasks to work on, ordered by urgency.
///
/// Filters out doing/done/deleted tasks, then sorts by urgency heuristic.
/// Always shows picker if tasks available, never auto-selects.
pub async fn run(config: &Config, project: Option<String>, no_sync: bool) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to query
    let project_ids = if let Some(proj) = project {
        vec![proj]
    } else {
        client
            .list_projects()
            .await?
            .into_iter()
            .map(|p| p.id)
            .collect()
    };

    // Collect all workable tasks
    let mut tasks = Vec::new();
    for project_id in &project_ids {
        let summaries = ctx.cache.list_tasks(project_id)?;
        for summary in summaries {
            // Filter: exclude done/deleted
            if summary.done.is_some() || summary.deleted.is_some() {
                continue;
            }

            // Load task and check if it's currently being worked on
            if let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id) {
                // Exclude tasks in "doing" state
                if task.current_work_state.as_deref() == Some("doing") {
                    continue;
                }
                tasks.push(task);
            }
        }
    }

    if tasks.is_empty() {
        return Err(crate::Error::UserFacing(
            "No tasks available to work on".to_string(),
        ));
    }

    // Sort by urgency (highest to lowest)
    tasks.sort_by(|a, b| {
        let now = Utc::now();
        calculate_urgency_score(a, &now).cmp(&calculate_urgency_score(b, &now))
    });

    // Show picker (always, even for 1 task)
    let selected_id = pick_next_task(&tasks)?;

    // Load the selected task and transition to "doing"
    let mut task = ctx.load_task(&client, &selected_id).await?;

    if task.current_work_state.as_deref() == Some("doing") {
        println!(
            "{} {} is already in progress",
            "ℹ".blue(),
            task.id[..8].cyan()
        );
        return Ok(());
    }

    let now = Utc::now();
    task.current_work_state = Some("doing".to_string());
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Add log entry for work state change
    task.log.push(LogEntry {
        timestamp: now,
        entry_type: LogEntryType::WorkStateChanged {
            state: WorkState::Doing,
        },
        source: crate::models::LogSource::User,
    });

    // Auto-set progress to 0% if not set
    if task.progress.is_none() {
        let old_progress = task.progress;
        task.progress = Some(0);
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ProgressChanged {
                from: old_progress,
                to: Some(0),
            },
            source: crate::models::LogSource::User,
        });
    }

    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!("{}", "✓ Task started!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Status:   {}", "doing".green());

    if !no_sync {
        if ctx.try_sync().await {
            println!("{}", "  ✓ Synced with server".green());
        } else {
            println!("{}", "  ⊙ Queued for sync".yellow());
        }
    }

    Ok(())
}

/// Calculate urgency score for sorting.
///
/// Returns tuple: (priority, deadline_urgency, impact, work_state, size, modified_timestamp)
/// Lower values = higher urgency (sorts first)
fn calculate_urgency_score(
    task: &Task,
    now: &chrono::DateTime<chrono::Utc>,
) -> (u8, i64, u8, u8, u8, i64) {
    // Priority: now=0, later=1
    let priority_score = match task.priority.as_str() {
        "now" => 0,
        _ => 1,
    };

    // Deadline urgency: seconds until deadline (negative if overdue)
    // No deadline = very far future (i64::MAX)
    let deadline_urgency = if let Some(ref deadline_str) = task.deadline {
        if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str) {
            let deadline_utc = deadline.with_timezone(&chrono::Utc);
            (deadline_utc - *now).num_seconds()
        } else {
            i64::MAX
        }
    } else {
        i64::MAX
    };

    // Impact: 1=highest urgency (Catastrophic), 5=lowest (Negligible)
    let impact_score = task.impact;

    // Work state: stopped=0 (has context), pending=1
    let work_state_score = match task.current_work_state.as_deref() {
        Some("stopped") => 0,
        _ => 1,
    };

    // Size: XS=0, S=1, M=2, L=3, XL=4 (smaller = higher urgency for quick wins)
    let size_score = match task.size.as_str() {
        "XS" => 0,
        "S" => 1,
        "M" => 2,
        "L" => 3,
        "XL" => 4,
        _ => 2, // default to M
    };

    // Modified timestamp: more recent = higher urgency (negate for ascending sort)
    let modified_timestamp =
        if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&task.modified) {
            -modified.timestamp()
        } else {
            0
        };

    (
        priority_score,
        deadline_urgency,
        impact_score,
        work_state_score,
        size_score,
        modified_timestamp,
    )
}

/// Interactive task picker showing urgency context.
fn pick_next_task(tasks: &[Task]) -> Result<String> {
    let items: Vec<String> = tasks
        .iter()
        .map(|t| {
            // Build urgency context (only add items with actual content)
            let mut context_parts: Vec<String> = Vec::new();

            // Priority indicator
            if t.priority == "now" {
                context_parts.push("🔴".to_string());
            }

            // Impact emoji
            match t.impact {
                1 => context_parts.push("🔥".to_string()),
                2 => context_parts.push("⚡".to_string()),
                _ => {}
            }

            // Deadline indicator
            if let Some(ref deadline_str) = t.deadline
                && let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(deadline_str)
            {
                let now = chrono::Utc::now();
                let deadline_utc = deadline.with_timezone(&chrono::Utc);

                if deadline_utc < now {
                    context_parts.push("⚠️  OVERDUE".red().to_string());
                } else {
                    let duration = deadline_utc - now;
                    let hours = duration.num_hours();
                    if hours < 48 {
                        context_parts.push(format!("⚠️  {}h", hours).yellow().to_string());
                    }
                }
            }

            // Work state indicator
            if t.current_work_state.as_deref() == Some("stopped") {
                context_parts.push("⏸️".to_string());
            }

            // Format with context if available
            if context_parts.is_empty() {
                format!("{} {}", t.id[..8].cyan(), t.title)
            } else {
                let context = context_parts.join(" ");
                format!("{} {} {}", t.id[..8].cyan(), t.title, context)
            }
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select next task to work on")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| crate::Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        return Err(crate::Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(tasks[idx].id.clone())
}
