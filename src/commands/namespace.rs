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

//! Namespace command implementations.

use colored::Colorize;

use crate::Result;
use crate::cache::{CachedNamespace, TaskCache};
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::models::{CreateNamespaceRequest, UpdateNamespaceRequest};
use crate::resolve;

/// Create a new namespace.
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

    // Resolve parent namespace if specified
    let parent_id = match parent {
        Some(ref p) => Some(resolve::resolve_namespace(&cache, p)?),
        None => None,
    };

    let req = CreateNamespaceRequest {
        name: name.to_string(),
        description,
        parent_id,
    };

    let ns = client.create_namespace(&req).await?;

    // Update local cache
    let cached = CachedNamespace {
        id: ns.id.clone(),
        name: ns.name.clone(),
        parent_id: ns.parent_id.clone(),
        deleted: None,
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: ns.labels.clone(),
    };
    cache.upsert_namespace(&cached)?;

    println!(
        "{}",
        format!("{} Namespace created successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:   {}", ns.id.cyan());
    println!("  Name: {}", ns.name);
    if let Some(desc) = &ns.description {
        println!("  Desc: {}", desc);
    }
    if let Some(pid) = &ns.parent_id {
        println!("  Parent: {}", pid);
    }

    Ok(())
}

/// List namespaces.
pub async fn list(config: &Config, all: bool) -> Result<()> {
    let client = Client::new(config)?;
    let namespaces = client.list_namespaces().await?;

    let filtered: Vec<_> = if all {
        namespaces
    } else {
        namespaces
            .into_iter()
            .filter(|ns| !ns.is_deleted())
            .collect()
    };

    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Build namespace_id -> [project_path, ...] map
    let mut links: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for ns in &filtered {
        let proj_ids = cache.get_linked_projects(&ns.id)?;
        if !proj_ids.is_empty() {
            let names: Vec<String> = proj_ids
                .iter()
                .filter_map(|pid| cache.get_project_path(pid).map(|path| path.join("/")).ok())
                .collect();
            if !names.is_empty() {
                links.insert(ns.id.clone(), names);
            }
        }
    }

    let icons = Icons::new(config.effective_icon_theme());
    crate::output::print_namespaces_with_links(&filtered, Some(&links), Some(&icons));
    Ok(())
}

/// Update a namespace.
pub async fn update(
    config: &Config,
    id: &str,
    description: Option<String>,
    parent: Option<String>,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, id)?;

    // Resolve parent: empty string means unparent, otherwise resolve
    let parent_id = match parent {
        Some(ref p) if p.is_empty() => Some(String::new()),
        Some(ref p) => Some(resolve::resolve_namespace(&cache, p)?),
        None => None,
    };

    let req = UpdateNamespaceRequest {
        name: None,
        description,
        parent_id,
    };

    let ns = client.update_namespace(&ns_id, &req).await?;

    // Update local cache
    let cached = CachedNamespace {
        id: ns.id.clone(),
        name: ns.name.clone(),
        parent_id: ns.parent_id.clone(),
        deleted: ns.deleted.clone(),
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: ns.labels.clone(),
    };
    cache.upsert_namespace(&cached)?;

    println!(
        "{}",
        format!("{} Namespace updated successfully!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:   {}", ns.id.cyan());
    println!("  Name: {}", ns.name);
    if let Some(desc) = &ns.description {
        println!("  Desc: {}", desc);
    }
    if let Some(pid) = &ns.parent_id {
        println!("  Parent: {}", pid);
    }

    Ok(())
}

/// Delete (soft-delete) a namespace.
pub async fn delete(config: &Config, id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, id)?;

    client.delete_namespace(&ns_id).await?;
    cache.soft_delete_namespace(&ns_id)?;

    println!(
        "{}",
        format!("{} Namespace '{}' deleted.", icons.success, ns_id)
            .green()
            .bold()
    );

    Ok(())
}

/// Restore a deleted namespace.
pub async fn restore(config: &Config, id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, id)?;

    let ns = client.restore_namespace(&ns_id).await?;

    let cached = CachedNamespace {
        id: ns.id.clone(),
        name: ns.name.clone(),
        parent_id: ns.parent_id.clone(),
        deleted: None,
        last_synced: Some(chrono::Utc::now().to_rfc3339()),
        labels: ns.labels.clone(),
    };
    cache.upsert_namespace(&cached)?;

    println!(
        "{}",
        format!("{} Namespace '{}' restored.", icons.success, ns.id)
            .green()
            .bold()
    );

    Ok(())
}

/// Link a project to a namespace.
pub async fn link(config: &Config, id: &str, project: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, id)?;
    let proj_id = resolve::resolve_project(&cache, project)?;

    client.link_namespace_project(&ns_id, &proj_id).await?;
    cache.link_namespace_project(&ns_id, &proj_id)?;

    println!(
        "{}",
        format!(
            "{} Linked project '{}' to namespace '{}'.",
            icons.success, proj_id, ns_id
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Unlink a project from a namespace.
pub async fn unlink(config: &Config, id: &str, project: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, id)?;
    let proj_id = resolve::resolve_project(&cache, project)?;

    client.unlink_namespace_project(&ns_id, &proj_id).await?;
    cache.unlink_namespace_project(&ns_id, &proj_id)?;

    println!(
        "{}",
        format!(
            "{} Unlinked project '{}' from namespace '{}'.",
            icons.success, proj_id, ns_id
        )
        .green()
        .bold()
    );

    Ok(())
}
