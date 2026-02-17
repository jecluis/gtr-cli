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

//! Create command implementation.

use chrono::Utc;
use colored::Colorize;
use uuid::Uuid;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::hierarchy;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::Task;
use crate::{output, threshold_cache, utils};

/// Create a new task (local-first with optional sync).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &Config,
    project: Option<String>,
    title: &str,
    edit_body: bool,
    priority: &str,
    size: &str,
    deadline: Option<String>,
    progress: Option<u8>,
    impact: Option<u8>,
    joy: Option<u8>,
    parent_id: Option<String>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());

    // Get project ID (may require server)
    let client = Client::new(config)?;
    let project_id = utils::resolve_project(&client, project).await?;

    // Validate deadline if provided
    let validated_deadline = if let Some(ref dl) = deadline {
        Some(utils::validate_deadline(dl)?)
    } else {
        None
    };

    let body = if edit_body {
        match crate::editor::edit_text(config, "") {
            Ok(content) => content,
            Err(crate::Error::InvalidInput(ref msg)) if msg == "Operation cancelled" => {
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    } else {
        String::new()
    };

    // Create task locally first
    let task_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Save locally first so we can resolve parent_id against cache
    let ctx = LocalContext::new(config, !no_sync)?;

    // Resolve parent_id if provided
    let resolved_parent = if let Some(ref pid) = parent_id {
        let full_pid = utils::resolve_task_id_from_cache(&ctx.cache, pid)?;
        // Validate parent exists
        if !ctx.cache.task_exists(&full_pid)? {
            return Err(crate::Error::UserFacing(format!(
                "Parent task not found: {pid}"
            )));
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
        Some(full_pid)
    } else {
        None
    };

    let task = Task {
        id: task_id.clone(),
        project_id: project_id.clone(),
        title: title.to_string(),
        body,
        priority: priority.to_string(),
        size: size.to_string(),
        created: now.clone(),
        modified: now,
        done: None,
        deleted: None,
        deadline: validated_deadline,
        version: 1,
        subtasks: vec![],
        custom: serde_json::Value::Object(serde_json::Map::new()),
        log: vec![],
        current_work_state: None,
        progress,
        impact: impact.unwrap_or(3),
        joy: joy.unwrap_or(5),
        parent_id: resolved_parent.clone(),
    };

    // Save locally
    ctx.storage.create_task(&project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!(
        "{}",
        format!("{} Task created locally!", icons.success)
            .green()
            .bold()
    );
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    println!(
        "  ID:       {}",
        output::format_full_id(&task.id, prefix_len)
    );
    println!("  Title:    {}", task.title);
    println!("  Priority: {}", task.priority);
    println!("  Size:     {}", task.size);

    if let Some(ref deadline_str) = task.deadline {
        println!("  Deadline: {}", deadline_str);
    }

    // Get impact label from cache (with fallback to defaults)
    let impact_label = threshold_cache::read_cache(config)
        .and_then(|cached| cached.impact_labels.get(&task.impact.to_string()).cloned())
        .or_else(|| {
            utils::default_impact_labels()
                .get(&task.impact.to_string())
                .cloned()
        })
        .unwrap_or_else(|| "Unknown".to_string());
    println!("  Impact:   {} ({})", impact_label, task.impact);

    if task.joy != 5 {
        let je = icons.joy_icon(task.joy);
        let joy_suffix = if je.is_empty() { "" } else { " " };
        println!("  Joy:      {}{}{}", task.joy, joy_suffix, je);
    }

    // Update parent's auto-progress if this is a subtask
    if task.parent_id.is_some() {
        hierarchy::update_ancestor_progress(&ctx.cache, &ctx.storage, &task.project_id, &task.id)?;
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
