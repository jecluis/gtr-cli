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

use chrono::{DateTime, Local, Utc};
use chrono_english::{Dialect, parse_date_string};
use colored::Colorize;
use dialoguer::Select;
use jiff::{Span, Zoned};

use crate::cache::{CachedNamespace, TaskCache};
use crate::client::Client;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::{Project, Task};
use crate::{Error, Result};

/// Build a breadcrumb string for a project (e.g., "work > research > ml").
///
/// Walks up the parent_id chain using the provided lookup map, displaying
/// human-readable names instead of raw UUIDs.
fn project_breadcrumb(
    project_id: &str,
    parent_map: &std::collections::HashMap<String, Option<String>>,
    name_map: &std::collections::HashMap<String, String>,
) -> String {
    let mut chain = vec![project_id.to_string()];
    let mut current = project_id.to_string();
    let mut seen = std::collections::HashSet::new();
    seen.insert(current.clone());

    while let Some(Some(pid)) = parent_map.get(&current) {
        if !seen.insert(pid.clone()) {
            break;
        }
        chain.push(pid.clone());
        current = pid.clone();
    }

    chain.reverse();

    // Filter out the meta-root (nil UUID) — it's an internal container,
    // not a user-visible project.
    const META_ROOT_ID: &str = "00000000-0000-0000-0000-000000000000";

    chain
        .iter()
        .filter(|id| id.as_str() != META_ROOT_ID)
        .map(|id| {
            name_map
                .get(id)
                .map(|s| s.as_str())
                .unwrap_or_else(|| id.as_str())
        })
        .collect::<Vec<_>>()
        .join(" > ")
}

/// Interactive project picker with breadcrumb display.
///
/// Shows projects sorted lexicographically by their hierarchy breadcrumb.
/// Auto-selects if only one project is provided.
pub fn pick_project(projects: &[Project]) -> Result<String> {
    if projects.is_empty() {
        return Err(Error::UserFacing("No projects to choose from.".to_string()));
    }

    if projects.len() == 1 {
        return Ok(projects[0].id.clone());
    }

    // Build parent and name lookups for breadcrumbs
    let parent_map: std::collections::HashMap<String, Option<String>> = projects
        .iter()
        .map(|p| (p.id.clone(), p.parent_id.clone()))
        .collect();
    let name_map: std::collections::HashMap<String, String> = projects
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    // Build breadcrumbs and sort lexicographically
    let mut entries: Vec<(String, String, Option<String>)> = projects
        .iter()
        .map(|p| {
            let breadcrumb = project_breadcrumb(&p.id, &parent_map, &name_map);
            (p.id.clone(), breadcrumb, p.description.clone())
        })
        .collect();
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    let items: Vec<String> = entries
        .iter()
        .map(|(_, breadcrumb, desc)| {
            if let Some(desc) = desc {
                format!("{} - {}", breadcrumb.cyan(), desc.dimmed())
            } else {
                breadcrumb.cyan().to_string()
            }
        })
        .collect();

    let selection = Select::new()
        .with_prompt("Select project")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| Error::InvalidInput(format!("Failed to read selection: {e}")))?;

    let Some(idx) = selection else {
        return Err(Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(entries[idx].0.clone())
}

/// Build a breadcrumb string for a namespace (e.g., "work > clyso").
fn namespace_breadcrumb(
    ns_id: &str,
    parent_map: &std::collections::HashMap<String, Option<String>>,
    name_map: &std::collections::HashMap<String, String>,
) -> String {
    let mut chain = vec![ns_id.to_string()];
    let mut current = ns_id.to_string();
    let mut seen = std::collections::HashSet::new();
    seen.insert(current.clone());

    while let Some(Some(pid)) = parent_map.get(&current) {
        if !seen.insert(pid.clone()) {
            break;
        }
        chain.push(pid.clone());
        current = pid.clone();
    }

    chain.reverse();

    chain
        .iter()
        .map(|id| {
            name_map
                .get(id)
                .map(|s| s.as_str())
                .unwrap_or_else(|| id.as_str())
        })
        .collect::<Vec<_>>()
        .join(" > ")
}

/// Interactive namespace picker with breadcrumb display.
pub fn pick_namespace(namespaces: &[CachedNamespace]) -> Result<String> {
    if namespaces.is_empty() {
        return Err(Error::UserFacing(
            "No namespaces to choose from.".to_string(),
        ));
    }

    if namespaces.len() == 1 {
        return Ok(namespaces[0].id.clone());
    }

    let parent_map: std::collections::HashMap<String, Option<String>> = namespaces
        .iter()
        .map(|ns| (ns.id.clone(), ns.parent_id.clone()))
        .collect();
    let name_map: std::collections::HashMap<String, String> = namespaces
        .iter()
        .map(|ns| (ns.id.clone(), ns.name.clone()))
        .collect();

    let mut entries: Vec<(String, String)> = namespaces
        .iter()
        .map(|ns| {
            let breadcrumb = namespace_breadcrumb(&ns.id, &parent_map, &name_map);
            (ns.id.clone(), breadcrumb)
        })
        .collect();
    entries.sort_by(|a, b| a.1.cmp(&b.1));

    let items: Vec<String> = entries
        .iter()
        .map(|(_, breadcrumb)| breadcrumb.cyan().to_string())
        .collect();

    let selection = Select::new()
        .with_prompt("Select namespace")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| Error::InvalidInput(format!("Failed to read selection: {e}")))?;

    let Some(idx) = selection else {
        return Err(Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(entries[idx].0.clone())
}

/// Resolve namespace: use provided, or auto-select if 1, or prompt picker.
pub fn resolve_namespace_interactive(
    cache: &TaskCache,
    provided: Option<String>,
) -> Result<String> {
    if let Some(ns) = provided {
        return crate::resolve::resolve_namespace(cache, &ns);
    }

    let namespaces = cache.list_namespaces()?;

    if namespaces.is_empty() {
        return Err(Error::UserFacing(
            "No namespaces found. Create one with 'gtr namespace create <name>'".to_string(),
        ));
    }

    if namespaces.len() == 1 {
        return Ok(namespaces[0].id.clone());
    }

    println!(
        "{}",
        "Multiple namespaces found. Please select one:".yellow()
    );
    pick_namespace(&namespaces)
}

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
    pick_project(&projects)
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

/// Resolve a potentially shortened task ID from the local cache.
///
/// Unlike `resolve_task_id`, this works offline using the SQLite cache.
/// Resolve a potentially shortened UUID from a list of known IDs.
///
/// If `short_id` is a full UUID (36 chars with 4 dashes), returns it as-is.
/// Otherwise prefix-matches against `all_ids`. `entity_name` is used in
/// error messages (e.g., "task", "document").
fn resolve_short_id(all_ids: &[String], short_id: &str, entity_name: &str) -> Result<String> {
    if short_id.len() == 36 && short_id.chars().filter(|&c| c == '-').count() == 4 {
        return Ok(short_id.to_string());
    }

    let matches: Vec<&String> = all_ids
        .iter()
        .filter(|id| id.starts_with(short_id))
        .collect();

    match matches.len() {
        0 => Err(Error::UserFacing(format!(
            "No {entity_name} found with ID prefix '{short_id}'"
        ))),
        1 => Ok(matches[0].clone()),
        _ => Err(Error::UserFacing(format!(
            "Ambiguous ID prefix '{short_id}' matches {} {entity_name}s. \
             Please provide more characters.",
            matches.len()
        ))),
    }
}

pub fn resolve_task_id_from_cache(
    cache: &crate::cache::TaskCache,
    short_id: &str,
) -> Result<String> {
    resolve_short_id(&cache.all_task_ids()?, short_id, "task")
}

/// Resolve a potentially shortened document ID from the local cache.
pub fn resolve_document_id(cache: &crate::cache::TaskCache, short_id: &str) -> Result<String> {
    resolve_short_id(&cache.all_document_ids()?, short_id, "document")
}

/// Parse a `[TYPE:]TARGET` string into (canonical_type, raw_id).
///
/// Recognised prefixes: `doc`/`document`, `task`, `proj`/`project`,
/// `ns`/`namespace`. When no prefix is present, `default_type` is used.
pub fn parse_typed_target<'a>(input: &'a str, default_type: &'a str) -> (&'a str, &'a str) {
    match input.split_once(':') {
        Some(("doc" | "document", id)) => ("document", id),
        Some(("task", id)) => ("task", id),
        Some(("proj" | "project", id)) => ("project", id),
        Some(("ns" | "namespace", id)) => ("namespace", id),
        _ => (default_type, input),
    }
}

/// Resolve a target ID based on entity type, using the local cache.
///
/// Dispatches to the right resolver:
/// - `"task"` -> prefix match against cached task IDs
/// - `"document"` -> prefix match against cached document IDs
/// - `"project"` -> name/path/UUID resolution
/// - `"namespace"` -> name/path/UUID resolution
pub fn resolve_target_id(
    cache: &crate::cache::TaskCache,
    raw_id: &str,
    entity_type: &str,
) -> Result<String> {
    match entity_type {
        "task" => resolve_task_id_from_cache(cache, raw_id),
        "document" => resolve_document_id(cache, raw_id),
        "project" => crate::resolve::resolve_project(cache, raw_id),
        "namespace" => crate::resolve::resolve_namespace(cache, raw_id),
        _ => Err(Error::InvalidInput(format!(
            "unknown target type '{entity_type}'"
        ))),
    }
}

/// Normalize time-of-day expressions that chrono-english can't handle.
///
/// chrono-english parses `8am`, `6pm` etc. but fails on `12pm`, `12am`,
/// `noon`, and `midnight`. This rewrites those to 24-hour format.
fn normalize_time_of_day(input: &str) -> String {
    let lower = input.to_lowercase();
    let replacements: &[(&str, &str)] = &[
        ("midnight", "0:00"),
        ("noon", "12:00"),
        ("12pm", "12:00"),
        ("12am", "0:00"),
    ];
    for &(pattern, replacement) in replacements {
        if let Some(pos) = lower.find(pattern) {
            let mut result = String::with_capacity(input.len());
            result.push_str(&input[..pos]);
            result.push_str(replacement);
            result.push_str(&input[pos + pattern.len()..]);
            return result;
        }
    }
    input.to_string()
}

/// Validate and normalize a deadline string to RFC3339 format.
///
/// Uses hybrid parsing strategy:
/// 1. Strict ISO 8601/RFC3339 formats (for programmatic use)
/// 2. Natural language via chrono-english (keywords, weekdays, times)
/// 3. Duration expressions via jiff (relative times, decimals, "ago")
///
/// Examples of valid input:
/// - Strict: "2026-02-15T08:00:00Z", "2026-02-15 08:00:00", "2026-02-15"
/// - Natural: "tomorrow", "next friday", "tomorrow 8am", "friday noon"
/// - Duration: "3 days", "2.5 weeks", "1 week 2 days ago"
pub fn validate_deadline(deadline_str: &str) -> Result<String> {
    // Try strict ISO 8601/RFC3339 parsing first (fast path for programmatic use)
    if let Ok(validated) = parse_strict_deadline(deadline_str) {
        return Ok(validated);
    }

    // Normalize time-of-day words that chrono-english can't handle:
    // "noon" → "12:00", "midnight" → "0:00", "12pm" → "12:00", "12am" → "0:00"
    let normalized = normalize_time_of_day(deadline_str);

    // Try chrono-english for natural language (keywords, weekdays, time-of-day)
    if let Ok(dt) = parse_date_string(&normalized, Local::now(), Dialect::Uk) {
        return Ok(dt.to_rfc3339());
    }

    // Try jiff::friendly for duration expressions (decimals, "ago", chained units)
    if let Ok(span) = deadline_str.parse::<Span>() {
        let now = Zoned::now();
        let deadline = now
            .checked_add(span)
            .map_err(|e| Error::InvalidInput(format!("Duration calculation failed: {}", e)))?;

        // Convert jiff::Zoned to RFC3339 string
        return deadline
            .strftime("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
            .parse()
            .map_err(|e| Error::InvalidInput(format!("Failed to format deadline: {}", e)));
    }

    // All parsers failed - provide comprehensive error message
    Err(Error::InvalidInput(format!(
        "Invalid deadline: '{}'\n\
        \n\
        Supported formats:\n\
        - ISO 8601: 2026-02-15T08:00:00Z, 2026-02-15 08:00:00, 2026-02-15\n\
        - Natural language: tomorrow, next friday, tomorrow 8am, last monday\n\
        - Duration: 3 days, 1 hour 30 minutes, 2.5 hours, 2 days ago",
        deadline_str
    )))
}

/// Parse strict ISO 8601/RFC3339 formats only (no natural language).
fn parse_strict_deadline(deadline_str: &str) -> Result<String> {
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

    Err(Error::InvalidInput(
        "Not a strict ISO 8601 format".to_string(),
    ))
}

/// Parse a threshold duration string (e.g., "12h", "48h", "7d") into seconds.
///
/// Supports:
/// - `Xh` — hours
/// - `Xd` — days
/// - `Xw` — weeks
///
/// Returns None for unparseable strings.
pub fn parse_threshold_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 2 {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: f64 = num_str.parse().ok()?;

    let secs = match unit {
        "h" => num * 3600.0,
        "d" => num * 86400.0,
        "w" => num * 604800.0,
        _ => return None,
    };

    Some(secs as i64)
}

/// System default deadline thresholds (same as server defaults).
pub fn default_thresholds() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    map.insert("XS".to_string(), "12h".to_string());
    map.insert("S".to_string(), "12h".to_string());
    map.insert("M".to_string(), "24h".to_string());
    map.insert("L".to_string(), "48h".to_string());
    map.insert("XL".to_string(), "7d".to_string());
    map
}

/// System default impact labels.
pub fn default_impact_labels() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    map.insert("1".to_string(), "Catastrophic".to_string());
    map.insert("2".to_string(), "Significant".to_string());
    map.insert("3".to_string(), "Neutral".to_string());
    map.insert("4".to_string(), "Minor".to_string());
    map.insert("5".to_string(), "Negligible".to_string());
    map
}

/// System default impact multipliers.
pub fn default_impact_multipliers() -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    map.insert("1".to_string(), 2.0);
    map.insert("2".to_string(), 1.5);
    map.insert("3".to_string(), 1.0);
    map.insert("4".to_string(), 0.5);
    map.insert("5".to_string(), 0.25);
    map
}

/// Interactive task picker with optional doing-first sorting and emoji indicators.
///
/// # Arguments
/// * `client` - Client for fetching projects
/// * `ctx` - LocalContext for loading tasks
/// * `prompt` - Prompt text for the picker
/// * `show_doing_first` - If true, sort tasks with "doing" state first
///
/// # Returns
/// Selected task ID
pub async fn pick_task(
    client: &Client,
    ctx: &LocalContext,
    prompt: &str,
    show_doing_first: bool,
    icons: &Icons,
) -> Result<String> {
    // Get all projects
    let projects = client.list_projects().await?;

    // Load all pending tasks
    let mut pending_tasks: Vec<Task> = Vec::new();
    for project in &projects {
        let summaries = ctx.cache.list_tasks(&project.id)?;
        for summary in summaries {
            if summary.done.is_none()
                && summary.deleted.is_none()
                && let Ok(task) = ctx.storage.load_task(&summary.id)
                && task.is_pending()
            {
                pending_tasks.push(task);
            }
        }
    }

    if pending_tasks.is_empty() {
        return Err(Error::UserFacing("No pending tasks found".to_string()));
    }

    // Sort: doing tasks first if requested
    if show_doing_first {
        pending_tasks.sort_by_key(|t| {
            let is_doing = t.current_work_state.as_deref() == Some("doing");
            (
                !is_doing,
                t.priority != "now",
                t.deadline.clone(),
                t.modified.clone(),
            )
        });
    }

    // Format display with emoji for doing tasks
    let items: Vec<String> = pending_tasks
        .iter()
        .map(|t| {
            let doing_prefix = if t.current_work_state.as_deref() == Some("doing") {
                "🔨 "
            } else {
                "   "
            };
            let progress_str = t.progress.map(|p| format!(" ({}%)", p)).unwrap_or_default();
            format!(
                "{}{} {}{}",
                doing_prefix,
                t.id[..8].cyan(),
                t.display_title(icons),
                progress_str.dimmed()
            )
        })
        .collect();

    let selection = Select::new()
        .with_prompt(prompt)
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        return Err(Error::UserFacing("Selection cancelled".to_string()));
    };

    Ok(pending_tasks[idx].id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_deadline_strict_iso8601() {
        // RFC3339 with timezone
        let result = validate_deadline("2026-02-15T08:00:00Z");
        assert!(result.is_ok());

        // RFC3339 with offset
        let result = validate_deadline("2026-02-15T08:00:00-05:00");
        assert!(result.is_ok());

        // Date and time (UTC assumed)
        let result = validate_deadline("2026-02-15 08:00:00");
        assert!(result.is_ok());

        // Date only (midnight UTC)
        let result = validate_deadline("2026-02-15");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_deadline_chrono_english() {
        // Keywords
        let result = validate_deadline("tomorrow");
        assert!(result.is_ok());

        let result = validate_deadline("today");
        assert!(result.is_ok());

        // Weekdays
        let result = validate_deadline("next friday");
        assert!(result.is_ok());

        let result = validate_deadline("last monday");
        assert!(result.is_ok());

        // With time
        let result = validate_deadline("tomorrow 8am");
        assert!(result.is_ok());

        let result = validate_deadline("next fri 6pm");
        assert!(result.is_ok());

        // Absolute dates
        let result = validate_deadline("1 April 2026");
        assert!(result.is_ok());

        let result = validate_deadline("April 1, 2026");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_deadline_jiff_friendly() {
        // Simple durations
        let result = validate_deadline("3 days");
        assert!(result.is_ok());

        let result = validate_deadline("2 weeks");
        assert!(result.is_ok());

        let result = validate_deadline("5 hours");
        assert!(result.is_ok());

        // Decimal durations (jiff supports fractional hours/minutes/seconds, not days)
        let result = validate_deadline("2.5 hours");
        assert!(result.is_ok());

        let result = validate_deadline("1.5h");
        assert!(result.is_ok());

        // Chained units
        let result = validate_deadline("1 week 2 days");
        assert!(result.is_ok());

        let result = validate_deadline("2 days 3 hours");
        assert!(result.is_ok());

        let result = validate_deadline("1 hour 30 minutes");
        assert!(result.is_ok());

        // "ago" syntax
        let result = validate_deadline("2 days ago");
        assert!(result.is_ok());

        let result = validate_deadline("3 hours ago");
        assert!(result.is_ok());

        let result = validate_deadline("1 week 2 days ago");
        assert!(result.is_ok());

        // Compact notation
        let result = validate_deadline("3d");
        assert!(result.is_ok());

        let result = validate_deadline("2h30m");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_deadline_invalid() {
        // Invalid formats should fail
        let result = validate_deadline("not a date");
        assert!(result.is_err());

        let result = validate_deadline("123abc");
        assert!(result.is_err());

        let result = validate_deadline("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_deadline_noon_midnight() {
        // "noon" and "midnight" as time-of-day
        assert!(validate_deadline("tomorrow noon").is_ok());
        assert!(validate_deadline("tomorrow midnight").is_ok());
        assert!(validate_deadline("Friday noon").is_ok());
        assert!(validate_deadline("next friday noon").is_ok());

        // "12pm" and "12am"
        assert!(validate_deadline("tomorrow 12pm").is_ok());
        assert!(validate_deadline("tomorrow 12am").is_ok());
        assert!(validate_deadline("Friday 12pm").is_ok());
        assert!(validate_deadline("next friday 12pm").is_ok());
    }

    #[test]
    fn test_validate_deadline_parser_precedence() {
        // "3 days" should be parsed by chrono-english first
        // (both parsers can handle it, but chrono-english wins)
        let result = validate_deadline("3 days");
        assert!(result.is_ok());

        // Verify result is in RFC3339 format
        let deadline = result.unwrap();
        assert!(DateTime::parse_from_rfc3339(&deadline).is_ok());
    }
}
