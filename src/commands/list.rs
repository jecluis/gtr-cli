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
use crate::local::LocalContext;
use crate::models::Task;
use crate::threshold_cache::{self, CachedThresholds};
use crate::{Result, output, utils};

/// List tasks (local-first from cache).
#[allow(clippy::too_many_arguments)]
pub async fn tasks(
    config: &Config,
    project: Option<Vec<String>>,
    priority: Option<String>,
    size: Option<String>,
    include_done: bool,
    include_deleted: bool,
    due_soon: bool,
    overdue: bool,
    limit: Option<u32>,
    reversed: bool,
    no_sync: bool,
    absolute_dates: bool,
    fancy: bool,
    verbose: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    // Determine which projects to query
    let project_ids = match project {
        None => {
            // No -P flag: show all projects (new default)
            client
                .list_projects()
                .await?
                .into_iter()
                .map(|p| p.id)
                .collect::<Vec<_>>()
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
            if let Ok(task) = ctx.storage.load_task(&summary.project_id, &summary.id) {
                all_tasks.push(task);
            }
        }
    }

    // Apply filters
    all_tasks.retain(|task| {
        // Filter by done/deleted status
        let status_ok = match (task.done.is_some(), task.deleted.is_some()) {
            (true, _) => include_done,
            (_, true) => include_deleted,
            _ => true,
        };

        // Filter by priority
        let priority_ok = priority
            .as_ref()
            .map(|p| task.priority == *p)
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
    let doing_tasks = sort_tasks(doing_tasks);
    let mut other_tasks = sort_tasks(other_tasks);

    // Reverse other tasks if flag is set
    if reversed {
        other_tasks.reverse();
    }

    // Fetch promotion thresholds for urgency indicators
    let cached = fetch_thresholds(config, &client, no_sync).await;

    // Apply deadline urgency emoji/color to task titles
    let doing_tasks = apply_deadline_urgency(doing_tasks, &cached);
    let other_tasks = apply_deadline_urgency(other_tasks, &cached);

    // Calculate prefix length based on ALL tasks (not just displayed ones)
    let prefix_len = crate::output::compute_min_prefix_len(&all_task_ids);

    output::print_tasks_grouped(
        &doing_tasks,
        &other_tasks,
        prefix_len,
        absolute_dates,
        fancy,
        verbose,
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

/// Sort tasks by priority (now > later), impact (1 first),
/// deadline (sooner first), modification time (descending: recently
/// touched at top), then creation time (ascending: new arrivals at bottom).
fn sort_tasks(mut tasks: Vec<Task>) -> Vec<Task> {
    tasks.sort_by(|a, b| {
        // First by priority (now < later for sorting, so now comes first)
        let priority_cmp = match (a.priority.as_str(), b.priority.as_str()) {
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

/// Fetch promotion thresholds (deadline + impact), checking cache first.
async fn fetch_thresholds(config: &Config, client: &Client, no_sync: bool) -> CachedThresholds {
    // Try local cache first
    if let Some(cached) = threshold_cache::read_cache(config) {
        if no_sync {
            return cached;
        }
    } else if no_sync {
        return CachedThresholds {
            deadline: utils::default_thresholds(),
            impact_labels: utils::default_impact_labels(),
            impact_multipliers: utils::default_impact_multipliers(),
        };
    }

    // Fetch from server and update cache
    match tokio::time::timeout(std::time::Duration::from_secs(2), client.get_user_config()).await {
        Ok(Ok(cfg)) => {
            let impact = cfg.promotion_thresholds.impact.as_ref();
            let cached = CachedThresholds {
                deadline: cfg.promotion_thresholds.deadline,
                impact_labels: impact
                    .map(|i| i.labels.clone())
                    .unwrap_or_else(utils::default_impact_labels),
                impact_multipliers: impact
                    .map(|i| i.multipliers.clone())
                    .unwrap_or_else(utils::default_impact_multipliers),
            };
            let _ = threshold_cache::write_cache(config, &cached);
            cached
        }
        _ => {
            // Fall back to cache, then defaults
            threshold_cache::read_cache(config).unwrap_or_else(|| CachedThresholds {
                deadline: utils::default_thresholds(),
                impact_labels: utils::default_impact_labels(),
                impact_multipliers: utils::default_impact_multipliers(),
            })
        }
    }
}

/// Apply deadline urgency indicators to task titles.
///
/// - Overdue: prepend boom emoji (no color change)
/// - Within 25% of threshold remaining: prepend warning emoji + amber title
///
/// Thresholds are scaled by the task's impact multiplier.
fn apply_deadline_urgency(mut tasks: Vec<Task>, cached: &CachedThresholds) -> Vec<Task> {
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
            // Overdue — boom emoji, no colorization
            task.title = format!("\u{1f4a5} {}", task.title);
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
                // Warning — emoji + amber title (no variation selector for better alignment)
                task.title = format!("\u{26a0} {}", task.title).yellow().to_string();
            }
        }
    }

    tasks
}
