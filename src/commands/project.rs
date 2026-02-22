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

//! Project command implementation.

use colored::Colorize;

use crate::Result;
use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::models::{CreateProjectRequest, UpdateProjectRequest};
use crate::output;

/// Create a new project.
pub async fn create(
    config: &Config,
    name: &str,
    description: Option<String>,
    parent: Option<String>,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Validate parent exists if specified
    if let Some(ref parent_id) = parent {
        match client.get_project(parent_id).await {
            Ok(p) => {
                if p.deleted.is_some() {
                    return Err(crate::Error::InvalidInput(format!(
                        "parent project '{}' is deleted",
                        parent_id
                    )));
                }
            }
            Err(crate::Error::ProjectNotFound(_)) => {
                return Err(crate::Error::InvalidInput(format!(
                    "parent project '{}' does not exist",
                    parent_id
                )));
            }
            Err(e) => return Err(e),
        }
    }

    // Use name verbatim as the project ID (no slugification)
    let req = CreateProjectRequest {
        id: name.to_string(),
        name: name.to_string(),
        description,
        parent_id: parent,
    };

    let project = client.create_project(&req).await?;

    // Update local cache
    let cached = crate::cache::CachedProject {
        id: project.id.clone(),
        name: project.name.clone(),
        parent_id: project.parent_id.clone(),
        deleted: None,
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: project.labels.clone(),
    };
    cache.upsert_project(&cached)?;

    println!(
        "{}",
        format!("{} Project created successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  Name:        {}", project.name.cyan().bold());
    println!("  ID:          {}", project.id.dimmed());
    if let Some(desc) = &project.description {
        println!("  Description: {}", desc);
    }
    if let Some(pid) = &project.parent_id {
        println!("  Parent:      {}", pid);
    }
    println!(
        "\nCreate tasks: {}",
        format!("gtr new <title> -P {}", project.name).dimmed()
    );

    Ok(())
}

/// Update a project.
pub async fn update(
    config: &Config,
    project_id: &str,
    description: Option<String>,
    parent: Option<String>,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let req = UpdateProjectRequest {
        name: None,
        description,
        parent_id: parent,
    };

    let project = client.update_project(project_id, &req).await?;

    // Update local cache
    let cached = crate::cache::CachedProject {
        id: project.id.clone(),
        name: project.name.clone(),
        parent_id: project.parent_id.clone(),
        deleted: project.deleted.clone(),
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: project.labels.clone(),
    };
    cache.upsert_project(&cached)?;

    println!(
        "{}",
        format!("{} Project updated successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  Name:        {}", project.name.cyan().bold());
    println!("  ID:          {}", project.id.dimmed());
    if let Some(desc) = &project.description {
        println!("  Description: {}", desc);
    }
    if let Some(pid) = &project.parent_id {
        println!("  Parent:      {}", pid);
    }

    Ok(())
}

/// Delete (soft-delete) a project.
pub async fn delete(config: &Config, project_id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Check locally for non-deleted tasks
    let active_count = cache.count_active_tasks_in_project(project_id)?;
    if active_count > 0 {
        return Err(crate::Error::InvalidInput(format!(
            "project '{}' has {} active task(s); delete or move them first",
            project_id, active_count
        )));
    }

    // Check for non-deleted subprojects
    let subs = cache.get_subprojects(project_id)?;
    if !subs.is_empty() {
        let sub_names: Vec<_> = subs.iter().map(|s| s.id.as_str()).collect();
        return Err(crate::Error::InvalidInput(format!(
            "project '{}' has subprojects: {}; delete them first (bottom-up)",
            project_id,
            sub_names.join(", ")
        )));
    }

    // Soft-delete on server
    client.delete_project(project_id).await?;

    // Update local cache
    cache.soft_delete_project(project_id)?;

    println!(
        "{}",
        format!("{} Project '{}' deleted.", icons.success, project_id)
            .green()
            .bold()
    );

    Ok(())
}

/// Restore a soft-deleted project.
pub async fn restore(config: &Config, project_id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let project = client.restore_project(project_id).await?;

    // Update local cache
    let cached = crate::cache::CachedProject {
        id: project.id.clone(),
        name: project.name.clone(),
        parent_id: project.parent_id.clone(),
        deleted: None,
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: project.labels.clone(),
    };
    cache.upsert_project(&cached)?;

    println!(
        "{}",
        format!("{} Project '{}' restored.", icons.success, project.id)
            .green()
            .bold()
    );

    Ok(())
}

/// List all projects.
pub async fn list(config: &Config, include_meta: bool) -> Result<()> {
    let client = Client::new(config)?;
    let projects = client.list_projects_all(include_meta).await?;

    output::print_projects(&projects);
    Ok(())
}

/// List all labels from `<root>` and every project in the hierarchy.
pub async fn label_list_all(config: &Config) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    // Fetch all projects including <root>
    let projects = client.list_projects_all(true).await?;
    if projects.is_empty() {
        println!("{}", format!("{} No projects found.", icons.info).dimmed());
        return Ok(());
    }

    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Build ID -> name map for display
    let name_map: std::collections::HashMap<&str, &str> = projects
        .iter()
        .map(|p| (p.id.as_str(), p.name.as_str()))
        .collect();

    let meta_root_id = "00000000-0000-0000-0000-000000000000";

    let mut any_labels = false;
    for project in &projects {
        let labels_with_source = cache.get_effective_labels_with_source(&project.id)?;
        if labels_with_source.is_empty() {
            continue;
        }
        any_labels = true;

        let counts = cache.count_tasks_by_label(&project.id)?;
        let count_map: std::collections::HashMap<String, i64> = counts.into_iter().collect();

        let header = if project.id == meta_root_id {
            "Global labels (Root):".to_string()
        } else {
            let display_name = cache
                .get_project_path(&project.id)
                .map(|path| path.join("/"))
                .unwrap_or_else(|_| project.name.clone());
            format!("{}:", display_name)
        };
        println!("{}", header.bold());

        for (label, source) in &labels_with_source {
            let count = count_map.get(label).copied().unwrap_or(0);
            if source == &project.id {
                println!("  {}  ({count} tasks)", label.cyan());
            } else {
                let origin = if source == meta_root_id {
                    "[global]".to_string()
                } else {
                    let source_name = name_map
                        .get(source.as_str())
                        .copied()
                        .unwrap_or(source.as_str());
                    format!("[inherited from {source_name}]")
                };
                println!("  {}  ({count} tasks)  {}", label.cyan(), origin.yellow());
            }
        }
        println!();
    }

    if !any_labels {
        println!(
            "{}",
            format!("{} No labels defined in any project.", icons.info).dimmed()
        );
    }

    Ok(())
}

/// List labels for a project, with task counts.
pub async fn label_list(config: &Config, project_id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;
    let project_id = crate::resolve::resolve_project(&cache, project_id)?;

    let meta_root_id = "00000000-0000-0000-0000-000000000000";

    let labels_with_source = cache.get_effective_labels_with_source(&project_id)?;
    if labels_with_source.is_empty() {
        let display_name = cache
            .get_project_path(&project_id)
            .map(|path| path.join("/"))
            .unwrap_or_else(|_| project_id.to_string());
        println!(
            "{}",
            format!("{} No labels in project '{}'.", icons.info, display_name).dimmed()
        );
        return Ok(());
    }

    let counts = cache.count_tasks_by_label(&project_id)?;
    let count_map: std::collections::HashMap<String, i64> = counts.into_iter().collect();

    let display_name = cache
        .get_project_path(&project_id)
        .map(|path| path.join("/"))
        .unwrap_or_else(|_| project_id.to_string());
    println!(
        "{}",
        format!("Labels for project '{}':", display_name).bold()
    );
    for (label, source) in &labels_with_source {
        let count = count_map.get(label).copied().unwrap_or(0);
        if source == &project_id {
            println!("  {}  ({count} tasks)", label.cyan());
        } else {
            let origin = if source == meta_root_id {
                "[global]".to_string()
            } else {
                let source_name = cache
                    .get_project_path(source)
                    .map(|path| path.join("/"))
                    .unwrap_or_else(|_| source.clone());
                format!("[inherited from {source_name}]")
            };
            println!("  {}  ({count} tasks)  {}", label.cyan(), origin.yellow());
        }
    }

    Ok(())
}

/// Add labels to a project registry.
pub async fn label_new(config: &Config, project_id: &str, labels: &[String]) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;
    let project_id = crate::resolve::resolve_project(&cache, project_id)?;

    // Validate all labels
    for label in labels {
        crate::labels::validate_label(label)?;
    }

    // Sync with server
    let project = client.create_project_labels(&project_id, labels).await?;

    // Update local cache
    cache.set_project_labels(&project_id, &project.labels)?;

    let display_name = cache
        .get_project_path(&project_id)
        .map(|path| path.join("/"))
        .unwrap_or_else(|_| project_id.clone());
    println!(
        "{}",
        format!(
            "{} Added label(s) {} to project '{}'.",
            icons.success,
            labels
                .iter()
                .map(|l| l.cyan().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            display_name
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Delete a label from a project (removes from all tasks too).
pub async fn label_delete(config: &Config, project_id: &str, label: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;
    let project_id = crate::resolve::resolve_project(&cache, project_id)?;

    // Confirm with user
    let confirm = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Delete label '{}' from project '{}'? This removes it from all tasks.",
            label, project_id
        ))
        .default(false)
        .interact()
        .unwrap_or(false);

    if !confirm {
        println!("Cancelled.");
        return Ok(());
    }

    // Sync with server
    let resp = client.delete_project_label(&project_id, label).await?;

    // Update local cache
    let mut labels = cache.get_project_labels(&project_id)?;
    labels.retain(|l| l != label);
    cache.set_project_labels(&project_id, &labels)?;
    cache.remove_label_from_tasks(&project_id, label)?;

    println!(
        "{}",
        format!(
            "{} Deleted label '{}' ({} tasks affected).",
            icons.success, label, resp.affected_tasks
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Rename a label in a project (updates all tasks too).
pub async fn label_rename(config: &Config, project_id: &str, old: &str, new: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;
    let project_id = crate::resolve::resolve_project(&cache, project_id)?;

    // Validate new label
    crate::labels::validate_label(new)?;

    // Sync with server
    let resp = client.rename_project_label(&project_id, old, new).await?;

    // Update local cache
    let mut labels = cache.get_project_labels(&project_id)?;
    for l in &mut labels {
        if l == old {
            *l = new.to_string();
        }
    }
    labels.sort();
    labels.dedup();
    cache.set_project_labels(&project_id, &labels)?;
    cache.rename_label_in_tasks(&project_id, old, new)?;

    println!(
        "{}",
        format!(
            "{} Renamed '{}' -> '{}' ({} tasks affected).",
            icons.success, old, new, resp.affected_tasks
        )
        .green()
        .bold()
    );

    Ok(())
}
