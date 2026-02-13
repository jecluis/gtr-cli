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

use tracing::{debug, info};

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntry, LogEntryType};
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
    progress: Option<u8>,
    impact: Option<u8>,
    no_sync: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    // Check if at least one field is provided
    if title.is_none()
        && !edit_body
        && priority.is_none()
        && size.is_none()
        && deadline.is_none()
        && progress.is_none()
        && impact.is_none()
    {
        return Err(Error::UserFacing(
            "No updates specified. Provide at least one field to update (--title, --body, --priority, --size, --deadline, --progress, or --impact)".to_string(),
        ));
    }

    // Initialize local context
    let ctx = LocalContext::new(config, !no_sync)?;

    // Load task from local storage (or fetch from server if not cached)
    let mut task = ctx.load_task(&client, &full_id).await?;

    let old_task = task.clone();

    // Edit body if requested (includes title as H1 header)
    let now = Utc::now();
    if edit_body {
        match crate::editor::edit_task_body(config, &task.title, &task.body) {
            Ok((new_title, new_body)) => {
                if task.body != new_body {
                    task.log.push(LogEntry {
                        timestamp: now,
                        entry_type: LogEntryType::BodyChanged,
                        source: crate::models::LogSource::User,
                    });
                    task.body = new_body;
                }
                // Update title if it changed in editor
                if let Some(title_from_editor) = new_title
                    && task.title != title_from_editor
                {
                    task.log.push(LogEntry {
                        timestamp: now,
                        entry_type: LogEntryType::TitleChanged {
                            from: task.title.clone(),
                            to: title_from_editor.clone(),
                        },
                        source: crate::models::LogSource::User,
                    });
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

    // Apply updates with logging
    if let Some(ref new_title) = title
        && task.title != *new_title
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::TitleChanged {
                from: task.title.clone(),
                to: new_title.clone(),
            },
            source: crate::models::LogSource::User,
        });
        task.title = new_title.clone();
    }

    if let Some(ref new_priority) = priority
        && task.priority != *new_priority
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::PriorityChanged {
                from: task.priority.clone(),
                to: new_priority.clone(),
            },
            source: crate::models::LogSource::User,
        });
        task.priority = new_priority.clone();
    }

    if let Some(ref new_size) = size
        && task.size != *new_size
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::SizeChanged {
                from: task.size.clone(),
                to: new_size.clone(),
            },
            source: crate::models::LogSource::User,
        });
        task.size = new_size.clone();
    }

    if let Some(ref new_deadline) = deadline {
        let new_deadline_parsed = if new_deadline == "none" {
            None
        } else {
            // Validate deadline format
            Some(crate::utils::validate_deadline(new_deadline)?)
        };

        if task.deadline != new_deadline_parsed {
            let old_deadline_dt = task
                .deadline
                .as_ref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let new_deadline_dt = new_deadline_parsed
                .as_ref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            task.log.push(LogEntry {
                timestamp: now,
                entry_type: LogEntryType::DeadlineChanged {
                    from: old_deadline_dt,
                    to: new_deadline_dt,
                },
                source: crate::models::LogSource::User,
            });
            task.deadline = new_deadline_parsed;
        }
    }

    if let Some(new_progress) = progress
        && task.progress != Some(new_progress)
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ProgressChanged {
                from: task.progress,
                to: Some(new_progress),
            },
            source: crate::models::LogSource::User,
        });
        task.progress = Some(new_progress);
    }

    if let Some(new_impact) = impact
        && task.impact != new_impact
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::ImpactChanged {
                from: task.impact,
                to: new_impact,
            },
            source: crate::models::LogSource::User,
        });
        task.impact = new_impact;
    }

    // Update metadata
    task.modified = now.to_rfc3339();
    task.version += 1;

    // Save locally
    info!(
        task_id = %task.id,
        version = task.version,
        deadline = ?task.deadline,
        priority = %task.priority,
        size = %task.size,
        "updating task locally"
    );
    debug!(
        task_id = %task.id,
        old_deadline = ?old_task.deadline,
        new_deadline = ?task.deadline,
        old_priority = %old_task.priority,
        new_priority = %task.priority,
        "field changes"
    );
    ctx.storage.update_task(&task.project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

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

    if let Some(new_priority) = priority
        && old_task.priority != new_priority
    {
        println!(
            "  {} {} → {}",
            "Priority:".bold(),
            old_task.priority.dimmed().strikethrough(),
            new_priority.green()
        );
    }

    if let Some(new_size) = size
        && old_task.size != new_size
    {
        println!(
            "  {} {} → {}",
            "Size:".bold(),
            old_task.size.dimmed().strikethrough(),
            new_size.green()
        );
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

    if progress.is_some() {
        let old_progress_str = old_task
            .progress
            .map(|p| format!("{}%", p))
            .unwrap_or_else(|| "none".to_string());
        let new_progress_str = task
            .progress
            .map(|p| format!("{}%", p))
            .unwrap_or_else(|| "none".to_string());

        if old_progress_str != new_progress_str {
            println!(
                "  {} {} → {}",
                "Progress:".bold(),
                old_progress_str.dimmed().strikethrough(),
                new_progress_str.green()
            );
        }
    }

    if impact.is_some() && old_task.impact != task.impact {
        println!(
            "  {} {} → {}",
            "Impact:".bold(),
            old_task.impact.to_string().dimmed().strikethrough(),
            task.impact.to_string().green()
        );
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
