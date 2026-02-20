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
use crate::{output, threshold_cache, url_fetch, utils};

/// Create a new task (local-first with optional sync).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: &Config,
    project: Option<String>,
    title: Option<String>,
    edit_body: bool,
    priority: &str,
    size: &str,
    deadline: Option<String>,
    progress: Option<u8>,
    impact: Option<u8>,
    joy: Option<u8>,
    parent_id: Option<String>,
    labels: Vec<String>,
    no_sync: bool,
    from_url: Option<String>,
    is_bookmark: bool,
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

    // Fetch URL content if --from or --bookmark was provided
    let url_content = if let Some(ref url_str) = from_url {
        match url_fetch::fetch_url_content(url_str).await {
            Some(content) => {
                println!(
                    "{}",
                    format!("  {} Fetched from URL", icons.success).green()
                );
                Some(content)
            }
            None => {
                println!(
                    "{}",
                    format!("  {} Could not fetch URL (using fallback)", icons.failure).yellow()
                );
                Some(url_fetch::fallback_content(url_str))
            }
        }
    } else {
        None
    };

    // Derive title: CLI args > fetched > URL fallback
    let final_title = match title {
        Some(t) => t,
        None => url_content
            .as_ref()
            .and_then(|c| c.title.clone())
            .unwrap_or_else(|| from_url.clone().unwrap_or_default()),
    };

    // Derive body: URL content body (with optional editor) or editor or empty
    let fetched_body = url_content
        .as_ref()
        .map(|c| c.body_markdown.clone())
        .unwrap_or_default();

    let body = if edit_body {
        match crate::editor::edit_text(config, &fetched_body) {
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
        fetched_body
    };

    // Build custom field (include source_url if from URL, is_bookmark flag)
    let mut custom_map = serde_json::Map::new();
    if let Some(ref url_str) = from_url {
        custom_map.insert(
            "source_url".to_string(),
            serde_json::Value::String(url_str.clone()),
        );
    }
    if is_bookmark {
        custom_map.insert("is_bookmark".to_string(), serde_json::Value::Bool(true));
    }

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

    // Validate and resolve labels
    let mut resolved_labels = Vec::new();
    if !labels.is_empty() {
        for label in &labels {
            crate::labels::validate_label(label)?;
        }

        // Check if labels exist in project's effective registry (own + inherited)
        let project_labels = ctx.cache.get_effective_labels(&project_id)?;
        let mut new_labels = Vec::new();
        for label in &labels {
            if !project_labels.contains(label) {
                let confirm = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "Label '{}' doesn't exist in project '{}'. Create it?",
                        label, project_id
                    ))
                    .default(true)
                    .interact()
                    .unwrap_or(false);
                if confirm {
                    new_labels.push(label.clone());
                }
            }
            resolved_labels.push(label.clone());
        }
        // Create missing labels in cache (sync will push later)
        if !new_labels.is_empty() {
            let mut all_labels = project_labels;
            all_labels.extend(new_labels);
            all_labels.sort();
            all_labels.dedup();
            ctx.cache.set_project_labels(&project_id, &all_labels)?;
        }
        resolved_labels.sort();
        resolved_labels.dedup();
    }

    let task = Task {
        id: task_id.clone(),
        project_id: project_id.clone(),
        title: final_title,
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
        custom: serde_json::Value::Object(custom_map),
        log: vec![],
        current_work_state: None,
        progress,
        impact: impact.unwrap_or(3),
        joy: joy.unwrap_or(5),
        parent_id: resolved_parent.clone(),
        labels: resolved_labels,
    };

    // Save locally
    ctx.storage.create_task(&task)?;
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
    println!("  Title:    {}", task.display_title(&icons));
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
        hierarchy::update_ancestor_progress(&ctx.cache, &ctx.storage, &task.id)?;
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
