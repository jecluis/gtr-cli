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

//! Status command — quick dashboard showing current state.

use chrono::Local;
use colored::Colorize;

use crate::Result;
use crate::cache::{ActiveTask, FeelsState, TaskCache};
use crate::commands::feels::{energy_description, focus_description};
use crate::config::Config;

/// Show a quick status dashboard.
pub async fn run(config: &Config, with_labels: bool) -> Result<()> {
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;
    let today = Local::now().date_naive();

    // -- Feels --
    print_feels(&cache, &today);

    // -- Active work --
    let active = cache.get_active_work_tasks()?;
    print_active_tasks(&active);

    // -- Counts --
    let overdue = cache.count_overdue()?;
    let due_today = cache.count_due_today()?;
    let pending_sync = cache.count_pending_sync()?;
    let done_today = cache.count_done_today()?;

    println!();
    print_counts(overdue, due_today, done_today, pending_sync);

    // -- Label distribution --
    if with_labels {
        print_label_stats(&cache)?;
    }

    Ok(())
}

fn print_feels(cache: &TaskCache, today: &chrono::NaiveDate) {
    match cache.get_today_feels(today) {
        Ok(Some(row)) if row.state == FeelsState::Set && row.energy > 0 => {
            println!(
                "{}  {} ({})",
                "Energy:".bold(),
                row.energy,
                energy_description(row.energy)
            );
            println!(
                " {}  {} ({})",
                "Focus:".bold(),
                row.focus,
                focus_description(row.focus)
            );
        }
        Ok(Some(row)) if row.state == FeelsState::Skipped => {
            println!("{}", "Feels: skipped for today".dimmed());
        }
        _ => {
            println!(
                "{}",
                "Feels: not set (use `gtr feels <energy> <focus>`)".dimmed()
            );
        }
    }
}

fn print_active_tasks(tasks: &[ActiveTask]) {
    if tasks.is_empty() {
        println!("\n{}", "No tasks in progress.".dimmed());
        return;
    }

    println!("\n{}", "Working on:".bold());
    for task in tasks {
        let state_badge = match task.work_state.as_str() {
            "doing" => "doing".green().bold().to_string(),
            "stopped" => "stopped".yellow().to_string(),
            other => other.to_string(),
        };
        let short_id = if task.id.len() > 8 {
            &task.id[..8]
        } else {
            &task.id
        };
        println!(
            "  {} {} {} [{}, {}]",
            state_badge,
            short_id.dimmed(),
            task.title,
            task.priority,
            task.size,
        );
    }
}

fn print_counts(overdue: i64, due_today: i64, done_today: i64, pending_sync: i64) {
    let overdue_str = if overdue > 0 {
        format!("Overdue: {}", overdue).red().bold().to_string()
    } else {
        format!("Overdue: {}", overdue).dimmed().to_string()
    };

    let due_str = if due_today > 0 {
        format!("Due today: {}", due_today).yellow().to_string()
    } else {
        format!("Due today: {}", due_today).dimmed().to_string()
    };

    let done_str = if done_today > 0 {
        format!("Done today: {}", done_today).green().to_string()
    } else {
        format!("Done today: {}", done_today).dimmed().to_string()
    };

    let sync_str = if pending_sync > 0 {
        format!("Pending sync: {}", pending_sync)
            .yellow()
            .to_string()
    } else {
        format!("Pending sync: {}", pending_sync)
            .dimmed()
            .to_string()
    };

    println!("{}  {}  {}  {}", overdue_str, due_str, done_str, sync_str);
}

fn print_label_stats(cache: &TaskCache) -> crate::Result<()> {
    let projects = cache.list_projects()?;
    let mut any_labels = false;

    for project in &projects {
        let counts = cache.count_tasks_by_label(&project.id)?;
        if counts.is_empty() {
            continue;
        }

        if !any_labels {
            println!("\n{}", "Labels:".bold());
            any_labels = true;
        }

        // Show project breadcrumb
        let breadcrumb = cache
            .get_project_path(&project.id)
            .unwrap_or_else(|_| vec![project.id.clone()]);
        println!("  {}", breadcrumb.join(" > ").cyan());

        // Sort by count descending
        let mut counts = counts;
        counts.sort_by(|a, b| b.1.cmp(&a.1));

        for (label, count) in &counts {
            let task_word = if *count == 1 { "task" } else { "tasks" };
            println!("    {}: {} {}", label, count, task_word.dimmed());
        }
    }

    if !any_labels {
        println!("\n{}", "No labels in any project.".dimmed());
    }

    Ok(())
}
