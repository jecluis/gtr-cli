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

//! List command implementation.

use chrono::{DateTime, Duration, Utc};
use colored::Colorize;

use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::Task;
use crate::threshold_cache::{self, CachedThresholds};
use crate::{Result, output, promotion, utils};

/// List tasks (local-first from cache).
#[allow(clippy::too_many_arguments)]
pub async fn tasks(
    config: &Config,
    project: Option<Vec<String>>,
    priority: Option<String>,
    size: Option<String>,
    with_done: bool,
    done: bool,
    with_deleted: bool,
    deleted: bool,
    all: bool,
    due_soon: bool,
    overdue: bool,
    limit: Option<u32>,
    reversed: bool,
    no_sync: bool,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
    for_task: Option<String>,
    recursive: bool,
    compact: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to query
    let mut project_ids = match project {
        None => {
            // No -P flag: show all projects (new default)
            let projects = client.list_projects().await?;
            // Refresh local project cache while we have the data
            let now = chrono::Utc::now().to_rfc3339();
            for p in &projects {
                let cached = crate::cache::CachedProject {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    parent_id: p.parent_id.clone(),
                    deleted: p.deleted.clone(),
                    last_synced: Some(now.clone()),
                };
                let _ = ctx.cache.upsert_project(&cached);
            }
            projects.into_iter().map(|p| p.id).collect::<Vec<_>>()
        }
        Some(vec) if vec.is_empty() => {
            // -P with no args: show picker
            vec![utils::resolve_project(&client, None).await?]
        }
        Some(vec) => {
            // -P with args: use specified projects
            vec
        }
    };

    // Expand to include subproject tasks when --recursive is used with -P
    // (but not when --for is set, which uses recursive for subtask trees)
    if recursive && for_task.is_none() {
        let mut expanded = project_ids.clone();
        for pid in &project_ids {
            if let Ok(descendants) = ctx.cache.get_project_descendants(pid) {
                expanded.extend(descendants);
            }
        }
        expanded.sort();
        expanded.dedup();
        project_ids = expanded;
    }

    // Collect ALL task IDs from cache (for consistent prefix highlighting)
    let all_task_ids = {
        let all_projects = client.list_projects().await.unwrap_or_default();
        let mut ids = Vec::new();
        for proj in &all_projects {
            if let Ok(summaries) = ctx.cache.list_tasks(&proj.id) {
                ids.extend(summaries.iter().map(|s| s.id.clone()));
            }
        }
        ids
    };

    // Load tasks from local cache and storage
    let mut all_tasks = Vec::new();

    for project_id in &project_ids {
        // Get task summaries from cache (includes project_id)
        let summaries = ctx.cache.list_tasks(project_id)?;

        for summary in summaries {
            // Load full task from storage
            if let Ok(task) = ctx.storage.load_task(&summary.id) {
                all_tasks.push(task);
            }
        }
    }

    // Filter to subtasks of a specific parent if --for is set
    if let Some(ref for_id) = for_task {
        let full_parent_id = utils::resolve_task_id(&client, for_id).await?;
        let allowed_ids: std::collections::HashSet<String> = if recursive {
            ctx.cache
                .get_all_descendants(&full_parent_id)?
                .into_iter()
                .collect()
        } else {
            ctx.cache
                .get_children(&full_parent_id)?
                .into_iter()
                .map(|c| c.id)
                .collect()
        };
        all_tasks.retain(|t| allowed_ids.contains(&t.id));

        // Show parent info header
        let parent_title = ctx
            .cache
            .get_task_title(&full_parent_id)?
            .unwrap_or_else(|| "?".to_string());
        let label = if recursive { "descendants" } else { "children" };
        println!(
            "{} {} {} ({})\n",
            "Subtasks of".bold(),
            full_parent_id[..8].cyan(),
            parent_title.dimmed(),
            label
        );
    }

    // Fetch promotion thresholds early — needed for filtering and sorting
    let cached = threshold_cache::fetch_thresholds(config, &client, no_sync).await;

    // Apply filters
    all_tasks.retain(|task| {
        // Filter by done/deleted status
        let status_ok = if done {
            // --done flag: show ONLY done tasks
            task.done.is_some()
        } else if deleted {
            // --deleted flag: show ONLY deleted tasks
            task.deleted.is_some()
        } else {
            // Normal filtering logic
            let include_done = all || with_done;
            let include_deleted = all || with_deleted;

            match (task.done.is_some(), task.deleted.is_some()) {
                (true, _) => include_done,
                (_, true) => include_deleted,
                _ => true,
            }
        };

        // Filter by priority (using effective priority so --priority now
        // includes tasks promoted by approaching deadlines)
        let priority_ok = priority
            .as_ref()
            .map(|p| promotion::effective_priority(task, &cached) == p.as_str())
            .unwrap_or(true);

        // Filter by size
        let size_ok = size.as_ref().map(|s| task.size == *s).unwrap_or(true);

        // Filter by deadline (due soon or overdue)
        let deadline_ok = if due_soon || overdue {
            if let Some(ref deadline_str) = task.deadline {
                if let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) {
                    let now = Utc::now();
                    let deadline_utc = deadline.with_timezone(&Utc);
                    if overdue {
                        deadline_utc < now
                    } else if due_soon {
                        deadline_utc < now + Duration::hours(48)
                    } else {
                        true
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            true
        };

        status_ok && priority_ok && size_ok && deadline_ok
    });

    // Apply limit if specified
    if let Some(lim) = limit {
        all_tasks.truncate(lim as usize);
    }

    // Sort and split tasks
    let (doing_tasks, other_tasks) = split_by_work_state(&mut all_tasks);

    // Sort both groups by priority then deadline
    let doing_tasks = sort_tasks(doing_tasks, &cached);
    let mut other_tasks = sort_tasks(other_tasks, &cached);

    // Reverse other tasks if flag is set
    if reversed {
        other_tasks.reverse();
    }

    // Apply deadline urgency icons/color to task titles
    let icons = Icons::new(config.effective_icon_theme());
    let doing_tasks = apply_deadline_urgency(doing_tasks, &cached, &icons);
    let other_tasks = apply_deadline_urgency(other_tasks, &cached, &icons);

    // Calculate prefix length based on ALL tasks (not just displayed ones)
    let prefix_len = crate::output::compute_min_prefix_len(&all_task_ids);

    // When --all or --with-done is used, separate done tasks and put them at the bottom
    let (all_tasks, doing_count) = if all || with_done {
        let (pending_other, done_other): (Vec<_>, Vec<_>) =
            other_tasks.into_iter().partition(|t| t.done.is_none());

        let doing_count = doing_tasks.len();
        let combined = [doing_tasks, pending_other, done_other].concat();
        (combined, doing_count)
    } else {
        // Normal behavior: doing first, then others
        let doing_count = doing_tasks.len();
        let combined = [doing_tasks, other_tasks].concat();
        (combined, doing_count)
    };

    let icons = Icons::new(config.effective_icon_theme());
    let project_paths = ctx.cache.build_project_paths(&all_tasks);
    output::print_tasks(
        &all_tasks,
        prefix_len,
        absolute_dates,
        fancy,
        verbose,
        Some(doing_count),
        &cached,
        &icons,
        compact,
        &project_paths,
    );
    Ok(())
}

/// Split tasks into doing and other groups.
fn split_by_work_state(tasks: &mut [Task]) -> (Vec<Task>, Vec<Task>) {
    let mut doing = Vec::new();
    let mut other = Vec::new();

    for task in tasks {
        if task.current_work_state.as_deref() == Some("doing") {
            doing.push(task.clone());
        } else {
            other.push(task.clone());
        }
    }

    (doing, other)
}

/// Sort tasks by effective priority (now > later), impact (1 first),
/// deadline (sooner first), modification time (descending: recently
/// touched at top), then creation time (ascending: new arrivals at bottom).
fn sort_tasks(mut tasks: Vec<Task>, thresholds: &CachedThresholds) -> Vec<Task> {
    tasks.sort_by(|a, b| {
        // First by effective priority (now < later for sorting, so now comes first)
        let a_prio = promotion::effective_priority(a, thresholds);
        let b_prio = promotion::effective_priority(b, thresholds);
        let priority_cmp = match (a_prio, b_prio) {
            ("now", "later") => std::cmp::Ordering::Less,
            ("later", "now") => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };

        if priority_cmp != std::cmp::Ordering::Equal {
            return priority_cmp;
        }

        // Then by impact (lower = higher impact, comes first)
        let impact_cmp = a.impact.cmp(&b.impact);
        if impact_cmp != std::cmp::Ordering::Equal {
            return impact_cmp;
        }

        // Then by deadline (sooner first, None last)
        let deadline_cmp = match (&a.deadline, &b.deadline) {
            (Some(a_deadline), Some(b_deadline)) => a_deadline.cmp(b_deadline),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };

        if deadline_cmp != std::cmp::Ordering::Equal {
            return deadline_cmp;
        }

        // Then by modification time (descending: recently touched at top)
        let modified_cmp = b.modified.cmp(&a.modified);
        if modified_cmp != std::cmp::Ordering::Equal {
            return modified_cmp;
        }

        // Finally by creation time (ascending: new arrivals at bottom)
        a.created.cmp(&b.created)
    });

    tasks
}

/// Apply deadline urgency indicators to task titles.
///
/// - Overdue: prepend boom emoji (no color change)
/// - Within 25% of threshold remaining: prepend warning emoji + amber title
///
/// Thresholds are scaled by the task's impact multiplier.
fn apply_deadline_urgency(
    mut tasks: Vec<Task>,
    cached: &CachedThresholds,
    icons: &Icons,
) -> Vec<Task> {
    let now = Utc::now();

    for task in &mut tasks {
        // Skip done/deleted tasks
        if task.done.is_some() || task.deleted.is_some() {
            continue;
        }

        let Some(ref deadline_str) = task.deadline else {
            continue;
        };
        let Ok(deadline) = DateTime::parse_from_rfc3339(deadline_str) else {
            continue;
        };

        let deadline_utc = deadline.with_timezone(&Utc);

        if deadline_utc < now {
            // Overdue — urgency icon, no colorization
            task.title = format!("{}{}", icons.overdue, task.title);
        } else {
            // Check if within 25% of threshold time remaining
            let threshold_str = cached.deadline.get(&task.size);
            let base_secs = threshold_str
                .and_then(|s| utils::parse_threshold_secs(s))
                .unwrap_or(86400); // fallback: 24h

            // Scale by impact multiplier
            let multiplier = cached
                .impact_multipliers
                .get(&task.impact.to_string())
                .copied()
                .unwrap_or(1.0);
            let effective_secs = (base_secs as f64 * multiplier) as i64;

            let warning_secs = effective_secs / 4;
            let remaining = (deadline_utc - now).num_seconds();

            if remaining <= warning_secs {
                // Warning — deadline icon + amber title
                task.title = format!("{}{}", icons.deadline_warning, task.title)
                    .yellow()
                    .to_string();
            }
        }
    }

    tasks
}
