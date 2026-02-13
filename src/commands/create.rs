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
use crate::local::LocalContext;
use crate::models::Task;
use crate::utils;

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
    no_sync: bool,
) -> Result<()> {
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
                println!("{}", "✗ Operation cancelled".yellow());
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
    };

    // Save locally
    let ctx = LocalContext::new(config, !no_sync)?;
    ctx.storage.create_task(&project_id, &task)?;
    ctx.cache.upsert_task(&task, true)?;

    println!("{}", "✓ Task created locally!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Priority: {}", task.priority);
    println!("  Size:     {}", task.size);

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
