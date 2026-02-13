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

//! Log command implementation.

use chrono::Local;
use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::local::LocalContext;
use crate::models::{LogEntryType, LogSource};
use crate::{Result, utils};

/// Display the change log for a task (from local storage).
pub async fn run(
    config: &Config,
    task_id: &str,
    work_only: bool,
    state_only: bool,
    no_sync: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    // Load from local storage (or fetch from server if not cached)
    let task = ctx.load_task(&client, &full_id).await?;

    if task.log.is_empty() {
        println!("{}", "No log entries found".yellow());
        return Ok(());
    }

    println!("\n{}", "═".repeat(60));
    println!("{}", format!("Task Log: {}", task.title).bold().green());
    println!("{}", "═".repeat(60));

    // Filter entries based on flags
    let filtered_entries: Vec<_> = task
        .log
        .iter()
        .filter(|entry| {
            if work_only {
                matches!(entry.entry_type, LogEntryType::WorkStateChanged { .. })
            } else if state_only {
                !matches!(entry.entry_type, LogEntryType::WorkStateChanged { .. })
            } else {
                true
            }
        })
        .collect();

    if filtered_entries.is_empty() {
        println!("\n{}", "No matching log entries found".yellow());
        return Ok(());
    }

    for entry in &filtered_entries {
        let local_time = entry.timestamp.with_timezone(&Local);
        let time_str = local_time.format("%Y-%m-%d %H:%M:%S").to_string();

        // Format source
        let source_str = match &entry.source {
            LogSource::User => "User".blue(),
            LogSource::System { reason } => format!("System ({})", reason).yellow(),
            LogSource::Import => "Import".cyan(),
        };

        // Format entry type
        let entry_str = match &entry.entry_type {
            LogEntryType::PriorityChanged { from, to } => {
                format!("Priority: {} → {}", from, to)
            }
            LogEntryType::DeadlineChanged { from, to } => {
                let from_str = from
                    .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "none".to_string());
                let to_str = to
                    .map(|d| d.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "none".to_string());
                format!("Deadline: {} → {}", from_str, to_str)
            }
            LogEntryType::StatusChanged { status } => {
                format!("Status: {:?}", status)
            }
            LogEntryType::SizeChanged { from, to } => {
                format!("Size: {} → {}", from, to)
            }
            LogEntryType::WorkStateChanged { state } => {
                format!("Work state: {:?}", state)
            }
            LogEntryType::TitleChanged { from, to } => {
                format!("Title: \"{}\" → \"{}\"", from, to)
            }
            LogEntryType::BodyChanged => "Body changed".to_string(),
            LogEntryType::ProgressChanged { from, to } => {
                let from_str = from
                    .map(|p| format!("{}%", p))
                    .unwrap_or_else(|| "none".to_string());
                let to_str = to
                    .map(|p| format!("{}%", p))
                    .unwrap_or_else(|| "none".to_string());
                format!("Progress: {} → {}", from_str, to_str)
            }
        };

        println!("\n  {} | {}", time_str.cyan(), source_str);
        println!("    {}", entry_str);
    }

    println!("\n{}", "═".repeat(60));
    println!("\n{} {} entries\n", "Total:".bold(), filtered_entries.len());

    Ok(())
}
