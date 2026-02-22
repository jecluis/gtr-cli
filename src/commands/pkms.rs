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

//! PKMS document command implementations.

use colored::Colorize;

use crate::Result;
use crate::cache::TaskCache;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::models::{AddReferenceRequest, CreateDocumentRequest, Document, UpdateDocumentRequest};
use crate::resolve;

/// Create a new document.
pub async fn create(
    config: &Config,
    namespace: Option<String>,
    title: Vec<String>,
    body: bool,
    labels: Vec<String>,
    parent: Option<String>,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Resolve namespace
    let ns_id = match namespace {
        Some(ref ns) => resolve::resolve_namespace(&cache, ns)?,
        None => {
            // Use the first available namespace
            let namespaces = cache.list_namespaces()?;
            let active: Vec<_> = namespaces.iter().filter(|ns| !ns.is_deleted()).collect();
            if active.is_empty() {
                return Err(crate::Error::InvalidInput(
                    "no namespaces available; create one first with `gtr namespace create`"
                        .to_string(),
                ));
            }
            if active.len() > 1 {
                return Err(crate::Error::InvalidInput(
                    "multiple namespaces exist; specify one with -N/--namespace".to_string(),
                ));
            }
            active[0].id.clone()
        }
    };

    let title_str = title.join(" ");

    // Optionally edit body in editor
    let content = if body {
        match crate::editor::edit_text(config, "") {
            Ok(text) => text,
            Err(crate::Error::InvalidInput(ref msg)) if msg == "Operation cancelled" => {
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    } else {
        String::new()
    };

    let req = CreateDocumentRequest {
        title: title_str,
        content,
        parent_id: parent,
        labels: if labels.is_empty() {
            None
        } else {
            Some(labels)
        },
    };

    let doc = client.create_document(&ns_id, &req).await?;
    cache.upsert_document(&doc, false)?;

    println!(
        "{}",
        format!("{} Document created!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:        {}", doc.id.cyan());
    println!("  Title:     {}", doc.title);
    println!("  Namespace: {}", doc.namespace_id);
    if !doc.labels.is_empty() {
        let label_strs: Vec<String> = doc.labels.iter().map(|l| l.cyan().to_string()).collect();
        println!("  Labels:    {}", label_strs.join(", "));
    }

    Ok(())
}

/// List documents.
pub async fn list(
    config: &Config,
    namespace: Option<String>,
    all: bool,
    with_labels: bool,
    labels: Vec<String>,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // If namespace specified, list from that namespace; otherwise list from all
    let docs: Vec<Document> = match namespace {
        Some(ref ns) => {
            let ns_id = resolve::resolve_namespace(&cache, ns)?;
            client.list_documents(&ns_id, all).await?
        }
        None => {
            // List from all namespaces
            let namespaces = client.list_namespaces().await?;
            let mut all_docs = Vec::new();
            for ns in &namespaces {
                if !all && ns.is_deleted() {
                    continue;
                }
                match client.list_documents(&ns.id, all).await {
                    Ok(docs) => all_docs.extend(docs),
                    Err(_) => continue,
                }
            }
            all_docs
        }
    };

    // Filter by labels if specified
    let filtered: Vec<&Document> = if labels.is_empty() {
        docs.iter().collect()
    } else {
        docs.iter()
            .filter(|d| labels.iter().any(|l| d.labels.contains(l)))
            .collect()
    };

    if filtered.is_empty() {
        println!("{}", format!("{} No documents found.", icons.info).dimmed());
        return Ok(());
    }

    // Print document table
    print_document_list(&filtered, with_labels);

    println!("\n{} {}", "Total:".bold(), filtered.len());
    Ok(())
}

/// Show a single document.
pub async fn show(config: &Config, doc_id: &str, _no_sync: bool, no_format: bool) -> Result<()> {
    let client = Client::new(config)?;
    let doc = client.get_document(doc_id).await?;

    let renderer = if no_format {
        crate::markdown::MarkdownRenderer::with_override(Some(false))
    } else {
        crate::markdown::MarkdownRenderer::with_override(None)
    };

    println!("\n{}", "=".repeat(80));
    println!("{}", doc.title.bold().green());
    println!("{}", "=".repeat(80));

    println!("\n{}", "Metadata:".bold());
    println!("  ID:        {}", doc.id.cyan());
    println!("  Namespace: {}", doc.namespace_id);

    if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&doc.created) {
        let local = created.with_timezone(&chrono::Local);
        println!("  Created:   {}", local.format("%Y-%m-%d %H:%M:%S"));
    }
    if let Ok(modified) = chrono::DateTime::parse_from_rfc3339(&doc.modified) {
        let local = modified.with_timezone(&chrono::Local);
        println!("  Modified:  {}", local.format("%Y-%m-%d %H:%M:%S"));
    }
    if let Some(ref deleted) = doc.deleted
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(deleted)
    {
        let local = dt.with_timezone(&chrono::Local);
        println!(
            "  {}",
            format!("Deleted:  {}", local.format("%Y-%m-%d %H:%M:%S")).red()
        );
    }
    if let Some(ref pid) = doc.parent_id {
        println!("  Parent:    {}", pid);
    }
    println!("  Version:   {}", doc.version);

    if !doc.labels.is_empty() {
        let label_strs: Vec<String> = doc.labels.iter().map(|l| l.cyan().to_string()).collect();
        println!("  Labels:    {}", label_strs.join(", "));
    }

    if !doc.references.is_empty() {
        println!("\n{}", "References:".bold());
        for r in &doc.references {
            println!(
                "  {} {} ({})",
                r.ref_type.dimmed(),
                r.target_id.cyan(),
                r.target_type
            );
        }
    }

    if !doc.content.is_empty() {
        println!("\n{}", "Content:".bold());
        println!("{}", "-".repeat(80));
        print!("{}", renderer.render(&doc.content));
    } else {
        println!("\n{}", "(No content)".italic().dimmed());
    }

    println!("{}\n", "=".repeat(80));

    Ok(())
}

/// Update a document.
#[allow(clippy::too_many_arguments)]
pub async fn update(
    config: &Config,
    doc_id: &str,
    title: Option<String>,
    body: bool,
    labels: Vec<String>,
    unlabels: Vec<String>,
    parent: Option<String>,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    // Get current document for label merging and body editing
    let current = client.get_document(doc_id).await?;

    // Handle body editing
    let new_content = if body {
        match crate::editor::edit_text(config, &current.content) {
            Ok(text) => Some(text),
            Err(crate::Error::InvalidInput(ref msg)) if msg == "Operation cancelled" => {
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    } else {
        None
    };

    // Merge labels: add new, remove unlabels
    let merged_labels = if !labels.is_empty() || !unlabels.is_empty() {
        let mut current_labels = current.labels.clone();
        for l in &labels {
            if !current_labels.contains(l) {
                current_labels.push(l.clone());
            }
        }
        current_labels.retain(|l| !unlabels.contains(l));
        Some(current_labels)
    } else {
        None
    };

    // Handle parent: empty string means unparent
    let parent_id = match parent {
        Some(ref p) if p.is_empty() => Some(String::new()),
        other => other,
    };

    let req = UpdateDocumentRequest {
        title,
        content: new_content,
        parent_id,
        labels: merged_labels,
    };

    let doc = client.update_document(doc_id, &req).await?;
    cache.upsert_document(&doc, false)?;

    println!(
        "{}",
        format!("{} Document updated!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:    {}", doc.id.cyan());
    println!("  Title: {}", doc.title);

    Ok(())
}

/// Delete (soft-delete) a document.
pub async fn delete(config: &Config, doc_id: &str, _recursive: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    client.delete_document(doc_id).await?;

    println!(
        "{}",
        format!("{} Document '{}' deleted.", icons.success, doc_id)
            .green()
            .bold()
    );

    Ok(())
}

/// Restore a deleted document.
pub async fn restore(config: &Config, doc_id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let doc = client.restore_document(doc_id).await?;
    cache.upsert_document(&doc, false)?;

    println!(
        "{}",
        format!("{} Document '{}' restored.", icons.success, doc.id)
            .green()
            .bold()
    );

    Ok(())
}

/// Move a document to a different namespace.
pub async fn move_doc(config: &Config, doc_id: &str, namespace: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let cache_path = config.cache_dir.join("index.db");
    let cache = TaskCache::open(&cache_path)?;

    let ns_id = resolve::resolve_namespace(&cache, namespace)?;
    let doc = client.move_document(doc_id, &ns_id).await?;
    cache.upsert_document(&doc, false)?;

    println!(
        "{}",
        format!("{} Document moved to namespace '{}'.", icons.success, ns_id)
            .green()
            .bold()
    );

    Ok(())
}

/// Add a reference from a document to another entity.
pub async fn link(
    config: &Config,
    doc_id: &str,
    target: &str,
    target_type: &str,
    ref_type: &str,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    let req = AddReferenceRequest {
        target_id: target.to_string(),
        target_type: target_type.to_string(),
        ref_type: ref_type.to_string(),
    };

    client.add_document_reference(doc_id, &req).await?;

    println!(
        "{}",
        format!(
            "{} Reference added: {} --[{}]--> {} ({})",
            icons.success, doc_id, ref_type, target, target_type
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Remove a reference from a document.
pub async fn unlink(config: &Config, doc_id: &str, target: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    client.remove_document_reference(doc_id, target).await?;

    println!(
        "{}",
        format!(
            "{} Reference removed: {} -/-> {}",
            icons.success, doc_id, target
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Show back-links (what references this document).
pub async fn backlinks(config: &Config, doc_id: &str) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    let refs = client.get_references(doc_id, "document").await?;

    if refs.forward.is_empty() && refs.back.is_empty() {
        println!(
            "{}",
            format!("{} No references found.", icons.info).dimmed()
        );
        return Ok(());
    }

    if !refs.forward.is_empty() {
        println!("{}", "Forward references:".bold());
        for r in &refs.forward {
            println!(
                "  {} {} ({}) [{}]",
                r.ref_type.dimmed(),
                r.target_id.cyan(),
                r.target_type,
                r.origin.dimmed()
            );
        }
    }

    if !refs.back.is_empty() {
        if !refs.forward.is_empty() {
            println!();
        }
        println!("{}", "Back-links:".bold());
        for r in &refs.back {
            println!(
                "  {} {} ({}) [{}]",
                r.ref_type.dimmed(),
                r.source_id.cyan(),
                r.source_type,
                r.origin.dimmed()
            );
        }
    }

    Ok(())
}

/// Print a simple document list.
fn print_document_list(docs: &[&Document], with_labels: bool) {
    use chrono_humanize::{Accuracy, HumanTime, Tense};

    for doc in docs {
        let short_id = &doc.id[..8.min(doc.id.len())];
        let modified_rel = chrono::DateTime::parse_from_rfc3339(&doc.modified)
            .map(|dt| {
                let ht = HumanTime::from(dt);
                ht.to_text_en(Accuracy::Rough, Tense::Past)
            })
            .unwrap_or_else(|_| "-".to_string());

        let deleted_tag = if doc.is_deleted() {
            " [deleted]".red().to_string()
        } else {
            String::new()
        };

        println!(
            "  {} {}{}  ({})",
            short_id.cyan(),
            doc.title,
            deleted_tag,
            modified_rel.dimmed()
        );

        if with_labels && !doc.labels.is_empty() {
            let label_strs: Vec<String> = doc.labels.iter().map(|l| l.cyan().to_string()).collect();
            println!("       {}", label_strs.join(", "));
        }
    }
}
