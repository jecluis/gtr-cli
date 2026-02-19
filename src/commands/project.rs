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
    };
    cache.upsert_project(&cached)?;

    println!(
        "{}",
        format!("{} Project created successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:          {}", project.id.cyan());
    println!("  Name:        {}", project.name);
    if let Some(desc) = &project.description {
        println!("  Description: {}", desc);
    }
    if let Some(pid) = &project.parent_id {
        println!("  Parent:      {}", pid);
    }
    println!(
        "\nCreate tasks: {}",
        format!("gtr new <title> -P {}", project.id).dimmed()
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
    };
    cache.upsert_project(&cached)?;

    println!(
        "{}",
        format!("{} Project updated successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:          {}", project.id.cyan());
    println!("  Name:        {}", project.name);
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
pub async fn list(config: &Config) -> Result<()> {
    let client = Client::new(config)?;
    let projects = client.list_projects().await?;

    output::print_projects(&projects);
    Ok(())
}
