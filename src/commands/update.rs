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
use crate::hierarchy;
use crate::icons::Icons;
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
    joy: Option<u8>,
    target_project: Option<String>,
    parent_id: Option<String>,
    unset: bool,
    recursive: bool,
    labels: Vec<String>,
    unlabels: Vec<String>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    // Validate --unset usage: empty-value flags require --unset
    let unset_deadline = deadline.as_deref() == Some("");
    let unset_parent = parent_id.as_deref() == Some("");

    if (unset_deadline || unset_parent) && !unset {
        return Err(Error::UserFacing(
            "Use --unset with -d/--for to clear fields (e.g. --unset -d)".to_string(),
        ));
    }

    // Normalize: convert empty sentinel to "none" for existing clear logic
    let deadline = if unset_deadline {
        Some("none".to_string())
    } else {
        deadline
    };
    let parent_id = if unset_parent {
        Some("none".to_string())
    } else {
        parent_id
    };

    // Check if at least one field is provided
    if title.is_none()
        && !edit_body
        && priority.is_none()
        && size.is_none()
        && deadline.is_none()
        && progress.is_none()
        && impact.is_none()
        && joy.is_none()
        && target_project.is_none()
        && parent_id.is_none()
        && labels.is_empty()
        && unlabels.is_empty()
    {
        return Err(Error::UserFacing(
            "No updates specified. Please provide at least one field to update.\n\n\
             Possible options:\n  \
               --body\n  \
               --deadline\n  \
               --for\n  \
               --impact\n  \
               --joy\n  \
               --priority\n  \
               --progress\n  \
               --project\n  \
               --size\n  \
               --title"
                .to_string(),
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
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
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

    if let Some(new_joy) = joy
        && task.joy != new_joy
    {
        task.log.push(LogEntry {
            timestamp: now,
            entry_type: LogEntryType::JoyChanged {
                from: task.joy,
                to: new_joy,
            },
            source: crate::models::LogSource::User,
        });
        task.joy = new_joy;
    }

    // Handle parent_id change
    if let Some(ref new_parent) = parent_id {
        if new_parent == "none" {
            task.parent_id = None;
        } else {
            let full_pid = utils::resolve_task_id_from_cache(&ctx.cache, new_parent)?;
            if full_pid == full_id {
                return Err(Error::UserFacing(
                    "A task cannot be its own parent".to_string(),
                ));
            }
            if !ctx.cache.task_exists(&full_pid)? {
                return Err(Error::UserFacing(format!(
                    "Parent task not found: {new_parent}"
                )));
            }
            if ctx.cache.would_create_cycle(&full_id, &full_pid)? {
                return Err(Error::UserFacing(
                    "Setting this parent would create a cycle".to_string(),
                ));
            }
            let depth = ctx.cache.get_depth(&full_pid)?;
            if depth >= 3 {
                eprintln!(
                    "{}",
                    format!(
                        "{} Warning: nesting depth > 3 can be hard to manage",
                        icons.overdue.trim()
                    )
                    .yellow()
                );
            }
            task.parent_id = Some(full_pid);
        }
    }

    // Handle label changes
    if !labels.is_empty() || !unlabels.is_empty() {
        let mut current_labels = task.labels.clone();

        // Validate new labels
        for label in &labels {
            crate::labels::validate_label(label)?;
        }

        // Check if labels exist in project registry; prompt to create if not
        if !labels.is_empty() {
            let project_labels = ctx.cache.get_project_labels(&task.project_id)?;
            let mut new_project_labels = Vec::new();
            for label in &labels {
                if !project_labels.contains(label) {
                    let confirm = dialoguer::Confirm::new()
                        .with_prompt(format!(
                            "Label '{}' doesn't exist in project '{}'. Create it?",
                            label, task.project_id
                        ))
                        .default(true)
                        .interact()
                        .unwrap_or(false);
                    if confirm {
                        new_project_labels.push(label.clone());
                    }
                }
                if !current_labels.contains(label) {
                    current_labels.push(label.clone());
                }
            }
            if !new_project_labels.is_empty() {
                let mut all_labels = project_labels;
                all_labels.extend(new_project_labels);
                all_labels.sort();
                all_labels.dedup();
                ctx.cache
                    .set_project_labels(&task.project_id, &all_labels)?;
            }
        }

        // Remove unlabeled
        for label in &unlabels {
            current_labels.retain(|l| l != label);
        }

        current_labels.sort();
        current_labels.dedup();
        task.labels = current_labels;
    }

    // Handle project move
    if let Some(ref new_project) = target_project
        && task.project_id != *new_project
    {
        // Check if task's labels exist in the target project
        if !task.labels.is_empty() {
            let target_labels = ctx.cache.get_project_labels(new_project)?;
            let mut labels_to_create = Vec::new();
            let mut labels_to_remove = Vec::new();

            for label in &task.labels {
                if !target_labels.contains(label) {
                    let confirm = dialoguer::Confirm::new()
                        .with_prompt(format!(
                            "Label '{}' doesn't exist in target project '{}'. Create it?",
                            label, new_project
                        ))
                        .default(true)
                        .interact()
                        .unwrap_or(false);
                    if confirm {
                        labels_to_create.push(label.clone());
                    } else {
                        labels_to_remove.push(label.clone());
                    }
                }
            }

            // Create missing labels in target project
            if !labels_to_create.is_empty() {
                let mut all_labels = target_labels;
                all_labels.extend(labels_to_create);
                all_labels.sort();
                all_labels.dedup();
                ctx.cache.set_project_labels(new_project, &all_labels)?;
            }

            // Remove labels the user declined to create
            if !labels_to_remove.is_empty() {
                task.labels.retain(|l| !labels_to_remove.contains(l));
            }
        }

        task.project_id = new_project.clone();
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
    ctx.storage.update_task(&task)?;
    ctx.cache.upsert_task(&task, true)?;

    // Update ancestor progress if this task has a parent
    if task.parent_id.is_some() {
        hierarchy::update_ancestor_progress(&ctx.cache, &ctx.storage, &task.id)?;
    }

    // Capture which recursive-eligible fields changed (before display takes ownership)
    let project_changed = target_project.is_some() && old_task.project_id != task.project_id;
    let priority_changed = priority.is_some() && old_task.priority != task.priority;
    let deadline_changed = deadline.is_some() && old_task.deadline != task.deadline;

    println!(
        "{}",
        format!("{} Task updated locally!", icons.success)
            .green()
            .bold()
    );
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = crate::output::compute_min_prefix_len(&all_ids);
    println!(
        "  ID:    {}",
        crate::output::format_full_id(&task.id, prefix_len)
    );
    println!("  Title: {}", task.display_title(&icons));

    // Show what changed with highlighting
    if let Some(new_title) = title {
        // Title changed via --title flag
        if old_task.title != new_title {
            println!(
                "  {} {} → {}",
                "Title:".bold(),
                old_task.display_title(&icons).dimmed().strikethrough(),
                new_title.green()
            );
        }
    } else if edit_body && old_task.title != task.title {
        // Title changed via editor
        println!(
            "  {} {} → {}",
            "Title:".bold(),
            old_task.display_title(&icons).dimmed().strikethrough(),
            task.display_title(&icons).green()
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

    if joy.is_some() && old_task.joy != task.joy {
        println!(
            "  {} {} → {}",
            "Joy:".bold(),
            old_task.joy.to_string().dimmed().strikethrough(),
            task.joy.to_string().green()
        );
    }

    if project_changed {
        println!(
            "  {} {} → {}",
            "Project:".bold(),
            old_task.project_id.dimmed().strikethrough(),
            task.project_id.cyan()
        );
    }

    if edit_body && old_task.body != task.body {
        println!("  {} {}", "Body:".bold(), "updated".green());
    }

    // Apply recursive updates to all descendants
    if recursive && (project_changed || priority_changed || deadline_changed) {
        let descendants = ctx.cache.get_all_descendants(&full_id)?;
        if !descendants.is_empty() {
            let count = apply_recursive_updates(
                &ctx,
                &descendants,
                if project_changed {
                    Some(task.project_id.as_str())
                } else {
                    None
                },
                if priority_changed {
                    Some(task.priority.as_str())
                } else {
                    None
                },
                if deadline_changed {
                    task.deadline.as_deref()
                } else {
                    None
                },
                deadline_changed && task.deadline.is_none(),
            )?;
            println!(
                "  {} Updated {} subtask{}",
                icons.success,
                count,
                if count == 1 { "" } else { "s" }
            );
        }
    }

    // Sync the project move on the server before general CRDT sync
    if !no_sync && project_changed {
        // Move the main task
        if let Err(e) = client.move_task(&full_id, &task.project_id).await {
            tracing::warn!(error = %e, "server move failed, queued for sync");
        }
        // Move descendants if recursive
        if recursive {
            let descendants = ctx.cache.get_all_descendants(&full_id)?;
            for desc_id in &descendants {
                if let Err(e) = client.move_task(desc_id, &task.project_id).await {
                    tracing::warn!(task_id = %desc_id, error = %e, "server move for subtask failed");
                }
            }
        }
    }

    // Attempt sync if enabled
    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!(
                "{}",
                format!("  {} Queued for sync (server unreachable)", icons.queued).yellow()
            );
        }
    }

    println!("\nView with: {}", format!("gtr show {}", task.id).dimmed());

    Ok(())
}

/// Apply recursive-eligible fields to a list of descendant tasks.
///
/// Returns the number of subtasks actually modified.
fn apply_recursive_updates(
    ctx: &LocalContext,
    descendant_ids: &[String],
    new_project: Option<&str>,
    new_priority: Option<&str>,
    new_deadline: Option<&str>,
    clear_deadline: bool,
) -> Result<usize> {
    let now = Utc::now();
    let mut count = 0;

    for desc_id in descendant_ids {
        let mut child = ctx.storage.load_task(desc_id)?;
        let mut changed = false;

        if let Some(project) = new_project
            && child.project_id != project
        {
            child.project_id = project.to_string();
            changed = true;
        }

        if let Some(priority) = new_priority
            && child.priority != priority
        {
            child.log.push(LogEntry {
                timestamp: now,
                entry_type: LogEntryType::PriorityChanged {
                    from: child.priority.clone(),
                    to: priority.to_string(),
                },
                source: crate::models::LogSource::User,
            });
            child.priority = priority.to_string();
            changed = true;
        }

        if let Some(deadline) = new_deadline
            && child.deadline.as_deref() != Some(deadline)
        {
            let old_dt = child
                .deadline
                .as_ref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            let new_dt = chrono::DateTime::parse_from_rfc3339(deadline)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc));
            child.log.push(LogEntry {
                timestamp: now,
                entry_type: LogEntryType::DeadlineChanged {
                    from: old_dt,
                    to: new_dt,
                },
                source: crate::models::LogSource::User,
            });
            child.deadline = Some(deadline.to_string());
            changed = true;
        } else if clear_deadline && child.deadline.is_some() {
            let old_dt = child
                .deadline
                .as_ref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));
            child.log.push(LogEntry {
                timestamp: now,
                entry_type: LogEntryType::DeadlineChanged {
                    from: old_dt,
                    to: None,
                },
                source: crate::models::LogSource::User,
            });
            child.deadline = None;
            changed = true;
        }

        if changed {
            child.modified = now.to_rfc3339();
            child.version += 1;
            ctx.storage.update_task(&child)?;
            ctx.cache.upsert_task(&child, true)?;
            count += 1;
        }
    }

    Ok(count)
}
