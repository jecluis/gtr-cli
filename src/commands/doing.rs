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

//! Doing command — list tasks currently in "doing" state.

use colored::Colorize;

use crate::Result;
use crate::cache::TaskCache;
use crate::config::Config;
use crate::models::Project;
use crate::output;
use crate::utils;

/// List tasks currently in "doing" state.
///
/// - No `-P`: all projects
/// - `-P` with no arg: show picker (only projects with doing tasks)
/// - `-P <id>`: filter to specified project and its subprojects
pub async fn run(config: &Config, project: Option<Vec<String>>) -> Result<()> {
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let active = cache.get_active_work_tasks()?;

    // Filter to only "doing" tasks (not "stopped")
    let doing: Vec<_> = active
        .into_iter()
        .filter(|t| t.work_state == "doing")
        .collect();

    // Determine project filter
    let filter_project = match project {
        None => None,
        Some(vec) if vec.is_empty() => {
            // -P with no args: pick from projects that have doing tasks
            if doing.is_empty() {
                println!("{}", "No tasks currently in doing state.".dimmed());
                return Ok(());
            }
            let candidates = doing_project_candidates(&cache, &doing);
            Some(utils::pick_project(&candidates)?)
        }
        Some(vec) => Some(vec.into_iter().next().unwrap_or_default()),
    };

    // Apply project filter (includes subprojects)
    let tasks: Vec<_> = if let Some(ref pid) = filter_project {
        let mut match_ids = vec![pid.clone()];
        if let Ok(descendants) = cache.get_project_descendants(pid) {
            match_ids.extend(descendants);
        }
        doing
            .into_iter()
            .filter(|t| match_ids.iter().any(|id| id == &t.project_id))
            .collect()
    } else {
        doing
    };

    if tasks.is_empty() {
        let scope = if let Some(pid) = filter_project {
            format!(" in project '{pid}'")
        } else {
            String::new()
        };
        println!(
            "{}",
            format!("No tasks currently in doing state{scope}.").dimmed()
        );
        return Ok(());
    }

    // Build project breadcrumbs from cache
    let project_ids: Vec<_> = tasks.iter().map(|t| t.project_id.as_str()).collect();
    let mut breadcrumbs = std::collections::HashMap::new();
    for pid in &project_ids {
        if !breadcrumbs.contains_key(*pid)
            && let Ok(chain) = cache.get_project_path(pid)
        {
            breadcrumbs.insert(pid.to_string(), chain.join(" > "));
        }
    }

    let all_task_ids = cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_task_ids);

    println!("{}", "Doing:".bold());
    for task in &tasks {
        let formatted_id = output::format_task_id(&task.id, prefix_len, true);

        let project_label = breadcrumbs
            .get(&task.project_id)
            .map(|b| b.as_str())
            .unwrap_or(&task.project_id);

        println!(
            "  {} {} [{}, {}] {}",
            formatted_id,
            task.title,
            task.priority,
            task.size,
            project_label.cyan(),
        );
    }

    Ok(())
}

/// Build the list of projects (and ancestors) that have doing tasks.
fn doing_project_candidates(cache: &TaskCache, doing: &[crate::cache::ActiveTask]) -> Vec<Project> {
    // Collect unique project IDs from doing tasks
    let mut project_ids: Vec<String> = doing.iter().map(|t| t.project_id.clone()).collect();
    project_ids.sort();
    project_ids.dedup();

    // Walk ancestors so that picking a parent project is possible
    let mut candidate_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for pid in &project_ids {
        if let Ok(chain) = cache.get_project_path(pid) {
            for ancestor in &chain {
                candidate_ids.insert(ancestor.clone());
            }
        } else {
            candidate_ids.insert(pid.clone());
        }
    }

    // Convert to Project structs for the shared picker
    candidate_ids
        .into_iter()
        .map(|id| {
            let (name, parent_id) = cache
                .get_project(&id)
                .ok()
                .flatten()
                .map(|p| (p.name, p.parent_id))
                .unwrap_or_else(|| (id.clone(), None));
            Project {
                id,
                name,
                description: None,
                deleted: None,
                parent_id,
            }
        })
        .collect()
}
