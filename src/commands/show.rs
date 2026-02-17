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

//! Show command implementation.

use colored::Colorize;
use dialoguer::Select;

use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::{Error, Result, output, threshold_cache, utils};

/// Show a specific task (local-first with optional refresh).
pub async fn run(
    config: &Config,
    task_id: &str,
    no_sync: bool,
    no_format: bool,
    no_wrap: bool,
    tree: bool,
) -> Result<()> {
    let client = Client::new(config)?;
    let full_id = utils::resolve_task_id(&client, task_id).await?;

    let ctx = LocalContext::new(config, !no_sync)?;

    if tree {
        return run_tree(config, &client, &ctx, &full_id, no_sync, no_format, no_wrap).await;
    }

    // Load from local storage (or fetch from server if not cached)
    let task = ctx.load_task(&client, &full_id).await?;

    let icons = Icons::new(config.effective_icon_theme());
    let cached = threshold_cache::fetch_thresholds(config, &client, no_sync).await;
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    output::print_task_details(
        config, &task, no_format, no_wrap, &cached, &icons, prefix_len,
    );

    // Show parent info
    if let Some(ref parent_id) = task.parent_id {
        let parent_title = ctx
            .cache
            .get_task_title(parent_id)?
            .unwrap_or_else(|| "?".to_string());
        // "Parent: " + icon + " " + "XXXX|XXXX" + " "
        let indent =
            8 + unicode_width::UnicodeWidthStr::width(icons.hierarchy_parent.as_str()) + 1 + 9 + 1;
        let prefix_colored = format!(
            "{} {} {} ",
            "Parent:".bold(),
            icons.hierarchy_parent.blue(),
            output::format_task_id(parent_id, prefix_len, true),
        );
        let wrapped = output::wrap_with_indent(&parent_title, 80, indent);
        print!("{}{}", prefix_colored, wrapped);
    }

    // Show children
    let children = ctx.cache.get_children(&full_id)?;
    if !children.is_empty() {
        println!("\n{}", "Subtasks:".bold());
        for child in &children {
            print_child_entry(child, prefix_len, &icons, &ctx.cache);
        }
    }

    // Try to refresh from server in background if sync enabled
    if !no_sync {
        match tokio::time::timeout(std::time::Duration::from_secs(2), client.get_task(&full_id))
            .await
        {
            Ok(Ok(fresh)) if fresh.version > task.version => {
                // Update local with fresh version
                ctx.storage.update_task(&fresh.project_id, &fresh)?;
                ctx.cache.upsert_task(&fresh, false)?;
                eprintln!(
                    "\n(Refreshed from server - version {} → {})",
                    task.version, fresh.version
                );
            }
            _ => {}
        }
    }

    Ok(())
}

struct TreeEntry {
    task_id: String,
    title: String,
    is_bookmark: bool,
    status: String,
    depth: usize,
    is_last: Vec<bool>,
}

fn task_status(cache: &TaskCache, task_id: &str, done: &Option<String>) -> String {
    if done.is_some() {
        return "done".to_string();
    }
    if let Ok(Some(ws)) = cache.get_work_state(task_id)
        && ws == "doing"
    {
        return "doing".to_string();
    }
    "pending".to_string()
}

fn build_tree(cache: &TaskCache, root_id: &str) -> Result<Vec<TreeEntry>> {
    let mut entries = Vec::new();

    let root_summary = cache.get_task_summary(root_id)?;
    let root_title = root_summary
        .as_ref()
        .map(|s| s.title.clone())
        .unwrap_or_else(|| "?".to_string());
    let root_bookmark = root_summary.as_ref().is_some_and(|s| s.is_bookmark);
    let root_done = cache.get_task_done(root_id)?;
    let root_status = task_status(cache, root_id, &root_done);

    entries.push(TreeEntry {
        task_id: root_id.to_string(),
        title: root_title,
        is_bookmark: root_bookmark,
        status: root_status,
        depth: 0,
        is_last: vec![],
    });

    build_children(cache, root_id, 1, &mut vec![], &mut entries)?;

    Ok(entries)
}

fn build_children(
    cache: &TaskCache,
    parent_id: &str,
    depth: usize,
    ancestor_last: &mut Vec<bool>,
    entries: &mut Vec<TreeEntry>,
) -> Result<()> {
    let children = cache.get_children(parent_id)?;
    let count = children.len();

    for (i, child) in children.iter().enumerate() {
        let is_last = i == count - 1;
        let mut is_last_flags = ancestor_last.clone();
        is_last_flags.push(is_last);

        let status = task_status(cache, &child.id, &child.done);

        entries.push(TreeEntry {
            task_id: child.id.clone(),
            title: child.title.clone(),
            is_bookmark: child.is_bookmark,
            status,
            depth,
            is_last: is_last_flags.clone(),
        });

        // Recurse into grandchildren
        ancestor_last.push(is_last);
        build_children(cache, &child.id, depth + 1, ancestor_last, entries)?;
        ancestor_last.pop();
    }

    Ok(())
}

fn format_tree_item(entry: &TreeEntry, icons: &Icons) -> String {
    let title = if entry.is_bookmark {
        format!("{}{}", icons.bookmark, entry.title)
    } else {
        entry.title.clone()
    };

    if entry.depth == 0 {
        // Root: no prefix
        let status = color_status(&entry.status);
        format!("{} {} [{}]", &entry.task_id[..8].cyan(), title, status)
    } else {
        let mut prefix = String::new();
        // Ancestor connectors (depth 1..depth-1)
        for d in 0..entry.depth - 1 {
            if entry.is_last[d] {
                prefix.push_str("   ");
            } else {
                prefix.push_str("│  ");
            }
        }
        // Current node connector
        if *entry.is_last.last().unwrap_or(&false) {
            prefix.push_str("└─ ");
        } else {
            prefix.push_str("├─ ");
        }

        let status = color_status(&entry.status);
        format!(
            "{}{} {} [{}]",
            prefix,
            &entry.task_id[..8].cyan(),
            title,
            status
        )
    }
}

fn color_status(status: &str) -> String {
    match status {
        "done" => "done".blue().to_string(),
        "doing" => "doing".yellow().to_string(),
        "pending" => "pending".green().to_string(),
        other => other.dimmed().to_string(),
    }
}

/// Print a single child task entry with icon, ID, status, and wrapped title.
fn print_child_entry(
    child: &crate::cache::TaskSummary,
    prefix_len: usize,
    icons: &Icons,
    cache: &TaskCache,
) {
    use unicode_width::UnicodeWidthStr;

    let status_label = task_status(cache, &child.id, &child.done);
    let status_colored = color_status(&status_label);

    // "  " + icon + " " + "XXXX|XXXX" + " [" + status + "] "
    let indent = 2
        + UnicodeWidthStr::width(icons.hierarchy_subtasks.as_str())
        + 1
        + 9
        + 2
        + status_label.len()
        + 2;
    let prefix_colored = format!(
        "  {} {} [{}] ",
        icons.hierarchy_subtasks.green(),
        output::format_task_id(&child.id, prefix_len, true),
        status_colored,
    );
    let wrapped = output::wrap_with_indent(&child.display_title(icons), 80, indent);
    print!("{}{}", prefix_colored, wrapped);
}

#[allow(clippy::too_many_arguments)]
async fn run_tree(
    config: &Config,
    client: &Client,
    ctx: &LocalContext,
    full_id: &str,
    no_sync: bool,
    no_format: bool,
    no_wrap: bool,
) -> Result<()> {
    // Ensure the root task is loaded
    let _task = ctx.load_task(client, full_id).await?;

    let entries = build_tree(&ctx.cache, full_id)?;
    let icons = Icons::new(config.effective_icon_theme());
    let items: Vec<String> = entries
        .iter()
        .map(|e| format_tree_item(e, &icons))
        .collect();

    let selection = Select::new()
        .with_prompt("Select task to view")
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|e| Error::InvalidInput(format!("Failed to read selection: {}", e)))?;

    let Some(idx) = selection else {
        eprintln!("{} Operation cancelled", icons.cancelled.yellow());
        return Ok(());
    };

    let selected_id = &entries[idx].task_id;

    // Show full details for the selected task (reuse normal show path)
    let task = ctx.load_task(client, selected_id).await?;
    let cached = threshold_cache::fetch_thresholds(config, client, no_sync).await;
    let all_ids = ctx.cache.all_task_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);
    output::print_task_details(
        config, &task, no_format, no_wrap, &cached, &icons, prefix_len,
    );

    // Show parent info
    if let Some(ref parent_id) = task.parent_id {
        let parent_title = ctx
            .cache
            .get_task_title(parent_id)?
            .unwrap_or_else(|| "?".to_string());
        // "Parent: " + icon + " " + "XXXX|XXXX" + " "
        let indent =
            8 + unicode_width::UnicodeWidthStr::width(icons.hierarchy_parent.as_str()) + 1 + 9 + 1;
        let prefix_colored = format!(
            "{} {} {} ",
            "Parent:".bold(),
            icons.hierarchy_parent.blue(),
            output::format_task_id(parent_id, prefix_len, true),
        );
        let wrapped = output::wrap_with_indent(&parent_title, 80, indent);
        print!("{}{}", prefix_colored, wrapped);
    }

    // Show children
    let children = ctx.cache.get_children(selected_id)?;
    if !children.is_empty() {
        println!("\n{}", "Subtasks:".bold());
        for child in &children {
            print_child_entry(child, prefix_len, &icons, &ctx.cache);
        }
    }

    Ok(())
}
