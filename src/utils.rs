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

//! Utility functions for the CLI.

use chrono::{DateTime, Utc};
use colored::Colorize;
use dialoguer::Select;

use crate::client::Client;
use crate::{Error, Result};

/// Resolve project ID: use provided, or auto-select if 1, or prompt.
pub async fn resolve_project(client: &Client, provided: Option<String>) -> Result<String> {
    // If project explicitly provided, use it
    if let Some(project_id) = provided {
        return Ok(project_id);
    }

    // Get all projects
    let projects = client.list_projects().await?;

    if projects.is_empty() {
        return Err(Error::UserFacing(
            "No projects found. Create one with 'gtr project create <name>'".to_string(),
        ));
    }

    // If only one project, use it automatically
    if projects.len() == 1 {
        return Ok(projects[0].id.clone());
    }

    // Multiple projects - prompt user
    println!("{}", "Multiple projects found. Please select one:".yellow());

    let items: Vec<String> = projects
        .iter()
        .map(|p| {
            if let Some(desc) = &p.description {
                format!("{} - {}", p.name.cyan(), desc.dimmed())
            } else {
                p.name.cyan().to_string()
            }
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select project")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    Ok(projects[selection].id.clone())
}

/// Resolve a potentially shortened task ID to a full UUID.
///
/// If the ID looks like a full UUID (36 chars), returns it as-is.
/// Otherwise, searches all tasks to find a unique prefix match.
pub async fn resolve_task_id(client: &Client, short_id: &str) -> Result<String> {
    // If it's already a full UUID format, return as-is
    if short_id.len() == 36 && short_id.chars().filter(|&c| c == '-').count() == 4 {
        return Ok(short_id.to_string());
    }

    // Try to use it directly first (in case server accepts it)
    if let Ok(task) = client.get_task(short_id).await {
        return Ok(task.id);
    }

    // Need to search for matching prefix - get all tasks
    // This is inefficient but works for now
    let all_projects = client.list_projects().await?;
    let mut matches = Vec::new();

    for project in all_projects {
        let tasks = client
            .list_tasks(&project.id, None, None, true, true, false, false, None)
            .await?;

        for task in tasks {
            if task.id.starts_with(short_id) {
                matches.push(task.id);
            }
        }
    }

    match matches.len() {
        0 => Err(Error::TaskNotFound(format!(
            "No task found with ID prefix '{}'",
            short_id
        ))),
        1 => Ok(matches[0].clone()),
        _ => Err(Error::UserFacing(format!(
            "Ambiguous ID prefix '{}' matches {} tasks. Please provide more characters.",
            short_id,
            matches.len()
        ))),
    }
}

/// Validate and normalize a deadline string to RFC3339 format.
///
/// Accepts various ISO 8601 / RFC3339 formats and returns a normalized
/// RFC3339 string with timezone. Returns an error with helpful message
/// if the format is invalid.
///
/// Examples of valid input:
/// - "2026-02-15T08:00:00Z"
/// - "2026-02-15T08:00:00-05:00"
/// - "2026-02-15 08:00:00"
pub fn validate_deadline(deadline_str: &str) -> Result<String> {
    // Try parsing as RFC3339 first
    if let Ok(dt) = DateTime::parse_from_rfc3339(deadline_str) {
        return Ok(dt.to_rfc3339());
    }

    // Try parsing as "YYYY-MM-DD HH:MM:SS" and assume UTC
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(deadline_str, "%Y-%m-%d %H:%M:%S") {
        let dt_utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
        return Ok(dt_utc.to_rfc3339());
    }

    // Try parsing as "YYYY-MM-DD" (date only, assume midnight UTC)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(deadline_str, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| Error::InvalidInput("Invalid date".to_string()))?;
        let dt_utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
        return Ok(dt_utc.to_rfc3339());
    }

    // Invalid format
    Err(Error::InvalidInput(format!(
        "Invalid deadline format: '{}'\n\
        \n\
        Supported formats:\n\
        - ISO 8601 with timezone: 2026-02-15T08:00:00Z\n\
        - ISO 8601 with offset: 2026-02-15T08:00:00-05:00\n\
        - Date and time (UTC): 2026-02-15 08:00:00\n\
        - Date only (midnight UTC): 2026-02-15",
        deadline_str
    )))
}
