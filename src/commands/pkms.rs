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

use chrono::Utc;
use uuid::Uuid;

use crate::Result;
use crate::cache::CachedDocument;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::local::LocalContext;
use crate::models::{Document, Namespace, Reference};
use crate::{output, resolve};

/// Create a new document (local-first with optional sync).
#[allow(clippy::too_many_arguments)]
pub async fn create(
    config: &Config,
    namespace: Option<String>,
    title: Vec<String>,
    body: bool,
    labels: Vec<String>,
    parent: Option<String>,
    _slug_prefix: Option<String>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let ctx = LocalContext::new(config, !no_sync)?;

    // Resolve namespace (picker if not specified)
    let ns_id = crate::utils::resolve_namespace_interactive(&ctx.cache, namespace)?;

    let title_str = title.join(" ");

    // Optionally edit body in editor (with title as H1 header)
    let (title_str, content) = if body {
        match crate::editor::edit_body(config, &title_str, "")? {
            crate::editor::EditorResult::Changed {
                title: new_title,
                body: new_body,
            } => (new_title.unwrap_or(title_str), new_body),
            crate::editor::EditorResult::Unchanged => (title_str, String::new()),
            crate::editor::EditorResult::Cancelled => {
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
                return Ok(());
            }
        }
    } else {
        (title_str, String::new())
    };

    // Resolve parent document if provided
    let resolved_parent = match parent {
        Some(ref p) => Some(crate::utils::resolve_document_id(&ctx.cache, p)?),
        None => None,
    };

    let now = Utc::now().to_rfc3339();
    let doc = Document {
        id: Uuid::new_v4().to_string(),
        namespace_id: ns_id.clone(),
        title: title_str,
        content,
        created: now.clone(),
        modified: now,
        deleted: None,
        version: 1,
        parent_id: resolved_parent,
        slug: String::new(),
        slug_aliases: vec![],
        labels,
        references: vec![],
        custom: serde_json::Value::Object(Default::default()),
    };

    // Save locally
    ctx.storage.create_document(&doc)?;
    ctx.cache.upsert_document(&doc, true)?;

    let all_ids = ctx.cache.all_document_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);

    let ns_display = ctx
        .cache
        .get_namespace_path(&doc.namespace_id)
        .ok()
        .map(|id_path| {
            id_path
                .iter()
                .filter_map(|id| ctx.cache.get_namespace(id).ok().flatten().map(|ns| ns.name))
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();

    println!(
        "{}",
        format!("{} Document created locally!", icons.success)
            .green()
            .bold()
    );
    println!(
        "  ID:        {}",
        output::format_full_id(&doc.id, prefix_len)
    );
    println!("  Title:     {}", doc.title);
    println!(
        "  Namespace: {} {}",
        ns_display.cyan().bold(),
        doc.namespace_id.dimmed()
    );
    if !doc.labels.is_empty() {
        let label_strs: Vec<String> = doc.labels.iter().map(|l| l.cyan().to_string()).collect();
        println!("  Labels:    {}", label_strs.join(", "));
    }

    // Attempt sync if enabled
    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!(
                "{}",
                format!("  {} Queued for sync (server unreachable)", icons.queued).yellow()
            );
        }
    }

    println!(
        "\nView with: {}",
        format!("gtr doc show {}", doc.id).dimmed()
    );

    Ok(())
}

/// List documents (local-first from cache).
pub async fn list(
    config: &Config,
    namespace: Option<String>,
    all: bool,
    with_labels: bool,
    labels: Vec<String>,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let ctx = LocalContext::new(config, true)?;

    let (namespaces_list, docs) = match namespace {
        Some(ref ns) => {
            let ns_id = resolve::resolve_namespace(&ctx.cache, ns)?;
            let cached_docs = ctx.cache.list_documents(&ns_id, all)?;
            (
                Vec::new(),
                cached_docs.into_iter().map(cached_to_document).collect(),
            )
        }
        None => {
            let cached_namespaces = ctx.cache.list_namespaces()?;
            let namespaces: Vec<Namespace> =
                cached_namespaces.iter().map(cached_to_namespace).collect();
            let mut all_docs = Vec::new();
            for ns in &cached_namespaces {
                if !all && ns.deleted.is_some() {
                    continue;
                }
                match ctx.cache.list_documents(&ns.id, all) {
                    Ok(d) => all_docs.extend(d.into_iter().map(cached_to_document)),
                    Err(_) => continue,
                }
            }
            (namespaces, all_docs)
        }
    };

    // Filter by labels if specified
    let filtered: Vec<Document> = if labels.is_empty() {
        docs
    } else {
        docs.into_iter()
            .filter(|d| labels.iter().any(|l| d.labels.contains(l)))
            .collect()
    };

    let all_ids = ctx.cache.all_document_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);

    if namespace.is_some() {
        output::print_documents_as_tree(&filtered, &icons, with_labels, prefix_len);
    } else {
        output::print_document_tree(&namespaces_list, &filtered, &icons, with_labels, prefix_len);
    }
    Ok(())
}

/// Show one or more documents (local-first with optional refresh).
pub async fn show(
    config: &Config,
    doc_ids: &[String],
    no_sync: bool,
    no_format: bool,
    no_wrap: bool,
    recursive: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let all_ids = ctx.cache.all_document_ids()?;
    let prefix_len = output::compute_min_prefix_len(&all_ids);

    let count = doc_ids.len();
    for (i, raw_id) in doc_ids.iter().enumerate() {
        let resolved = crate::utils::resolve_document_id(&ctx.cache, raw_id)?;
        show_one_document(
            &client, &ctx, &icons, no_format, no_wrap, recursive, &resolved, prefix_len, 0, "",
        )
        .await?;

        // Separator between top-level entities
        if i + 1 < count {
            println!("\n{}", "─".repeat(60));
        }
    }
    Ok(())
}

/// Display a single document with optional recursive child expansion.
///
/// `tree_prefix` is the accumulated connector continuation from all
/// ancestor levels.
#[allow(clippy::too_many_arguments)]
async fn show_one_document(
    client: &Client,
    ctx: &LocalContext,
    icons: &Icons,
    no_format: bool,
    no_wrap: bool,
    recursive: bool,
    doc_id: &str,
    prefix_len: usize,
    depth: usize,
    tree_prefix: &str,
) -> Result<()> {
    let indent_str = if depth > 0 {
        format!("{}│ ", tree_prefix)
    } else {
        String::new()
    };

    let doc = ctx.load_document(client, doc_id).await?;

    let ns_display = ctx
        .cache
        .get_namespace_path(&doc.namespace_id)
        .ok()
        .map(|id_path| {
            id_path
                .iter()
                .filter_map(|id| ctx.cache.get_namespace(id).ok().flatten().map(|ns| ns.name))
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();

    output::print_document_detail(
        &doc,
        icons,
        no_format,
        no_wrap,
        prefix_len,
        &ns_display,
        &indent_str,
    );

    // Children: either recurse into them or show inline summary
    if recursive {
        let children = ctx.cache.get_document_children(doc_id)?;
        for (i, child) in children.iter().enumerate() {
            let is_last = i == children.len() - 1;
            let connector = if is_last { "└─" } else { "├─" };
            let continuation = if is_last { "   " } else { "│  " };

            println!(
                "{}{} Child: {}",
                tree_prefix,
                connector,
                output::format_task_id(&child.id, prefix_len, true),
            );

            let child_tree = format!("{}{}", tree_prefix, continuation);
            Box::pin(show_one_document(
                client,
                ctx,
                icons,
                no_format,
                no_wrap,
                recursive,
                &child.id,
                prefix_len,
                depth + 1,
                &child_tree,
            ))
            .await?;

            if !is_last {
                println!("{}", child_tree.trim_end());
            }
        }
    }

    Ok(())
}

/// Update a document (local-first with optional sync).
#[allow(clippy::too_many_arguments)]
pub async fn update(
    config: &Config,
    doc_id: &str,
    title: Option<String>,
    body: bool,
    labels: Vec<String>,
    unlabels: Vec<String>,
    parent: Option<String>,
    _slug_prefix: Option<String>,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    // Load current document from local storage (or fetch from server)
    let mut doc = ctx.load_document(&client, &doc_id).await?;

    // Handle body editing (with title as H1 header)
    let (editor_title, new_content) = if body {
        match crate::editor::edit_body(config, &doc.title, &doc.content)? {
            crate::editor::EditorResult::Changed {
                title,
                body: new_body,
            } => (title, Some(new_body)),
            crate::editor::EditorResult::Unchanged => (None, None),
            crate::editor::EditorResult::Cancelled => {
                println!(
                    "{}",
                    format!("{} Operation cancelled", icons.cancelled).yellow()
                );
                return Ok(());
            }
        }
    } else {
        (None, None)
    };

    // Merge labels: add new, remove unlabels
    let merged_labels = if !labels.is_empty() || !unlabels.is_empty() {
        let mut current_labels = doc.labels.clone();
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
        Some(p) => Some(crate::utils::resolve_document_id(&ctx.cache, &p)?),
        None => None,
    };

    // --title flag takes precedence over title changed in editor
    let effective_title = title.or(editor_title);

    // Detect whether anything actually changed
    let has_changes = effective_title.is_some()
        || new_content.is_some()
        || parent_id.is_some()
        || merged_labels.is_some();

    if !has_changes {
        println!("{}", format!("{} No changes to save.", icons.info).yellow());
        return Ok(());
    }

    // Apply changes to the document
    if let Some(ref t) = effective_title {
        doc.title = t.clone();
    }
    if let Some(ref c) = new_content {
        doc.content = c.clone();
    }
    if let Some(ref pid) = parent_id {
        if pid.is_empty() {
            doc.parent_id = None;
        } else {
            doc.parent_id = Some(pid.clone());
        }
    }
    if let Some(lbls) = merged_labels {
        doc.labels = lbls;
    }

    doc.modified = Utc::now().to_rfc3339();
    doc.version += 1;

    // Save locally
    ctx.storage.update_document(&doc)?;
    ctx.cache.upsert_document(&doc, true)?;

    println!(
        "{}",
        format!("{} Document updated locally!", icons.success)
            .green()
            .bold()
    );
    println!("  ID:    {}", doc.id.cyan());
    println!("  Title: {}", doc.title);
    if !doc.slug.is_empty() {
        println!("  Slug:  {}", doc.slug.cyan());
    }

    // Attempt sync if enabled
    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!(
                "{}",
                format!("  {} Queued for sync (server unreachable)", icons.queued).yellow()
            );
        }
    }

    Ok(())
}

/// Delete (soft-delete) a document (local-first with optional sync).
pub async fn delete(config: &Config, doc_id: &str, _recursive: bool, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = &crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    let mut doc = ctx.load_document(&client, doc_id).await?;
    doc.deleted = Some(Utc::now().to_rfc3339());
    doc.modified = Utc::now().to_rfc3339();
    doc.version += 1;

    ctx.storage.update_document(&doc)?;
    ctx.cache.upsert_document(&doc, true)?;

    println!(
        "{}",
        format!("{} Document '{}' deleted locally.", icons.success, doc_id)
            .green()
            .bold()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Restore a deleted document (local-first with optional sync).
pub async fn restore(config: &Config, doc_id: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    let mut doc = ctx.load_document(&client, &doc_id).await?;
    doc.deleted = None;
    doc.modified = Utc::now().to_rfc3339();
    doc.version += 1;

    ctx.storage.update_document(&doc)?;
    ctx.cache.upsert_document(&doc, true)?;

    println!(
        "{}",
        format!("{} Document '{}' restored locally.", icons.success, doc.id)
            .green()
            .bold()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Move a document to a different namespace (local-first with optional sync).
pub async fn move_doc(config: &Config, doc_id: &str, namespace: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;

    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;
    let ns_id = resolve::resolve_namespace(&ctx.cache, namespace)?;

    let mut doc = ctx.load_document(&client, &doc_id).await?;
    doc.namespace_id = ns_id.clone();
    doc.modified = Utc::now().to_rfc3339();
    doc.version += 1;

    ctx.storage.update_document(&doc)?;
    ctx.cache.upsert_document(&doc, true)?;

    println!(
        "{}",
        format!(
            "{} Document moved to namespace '{}' locally.",
            icons.success, ns_id
        )
        .green()
        .bold()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Add a reference from a document to another entity (local-first with optional sync).
pub async fn link(
    config: &Config,
    doc_id: &str,
    target: &str,
    ref_type: &str,
    no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    let (target_type, raw_id) = crate::utils::parse_typed_target(target, "document");
    let target_id = crate::utils::resolve_target_id(&ctx.cache, raw_id, target_type)?;

    let mut doc = ctx.load_document(&client, &doc_id).await?;

    // Add reference if not already present
    let new_ref = Reference {
        target_id: target_id.clone(),
        target_type: target_type.to_string(),
        ref_type: ref_type.to_string(),
    };
    if !doc.references.contains(&new_ref) {
        doc.references.push(new_ref);
        doc.modified = Utc::now().to_rfc3339();
        doc.version += 1;

        ctx.storage.update_document(&doc)?;
        ctx.cache.upsert_document(&doc, true)?;
    }

    println!(
        "{}",
        format!(
            "{} Reference added: {} --[{}]--> {} ({})",
            icons.success, doc_id, ref_type, target_id, target_type
        )
        .green()
        .bold()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Remove a reference from a document (local-first with optional sync).
pub async fn unlink(config: &Config, doc_id: &str, target: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    let (target_type, raw_id) = crate::utils::parse_typed_target(target, "document");
    let target_id = crate::utils::resolve_target_id(&ctx.cache, raw_id, target_type)?;

    let mut doc = ctx.load_document(&client, &doc_id).await?;

    // Remove matching references
    let before_len = doc.references.len();
    doc.references.retain(|r| r.target_id != target_id);

    if doc.references.len() < before_len {
        doc.modified = Utc::now().to_rfc3339();
        doc.version += 1;

        ctx.storage.update_document(&doc)?;
        ctx.cache.upsert_document(&doc, true)?;
    }

    println!(
        "{}",
        format!(
            "{} Reference removed: {} -/-> {}",
            icons.success, doc_id, target_id
        )
        .green()
        .bold()
    );

    if !no_sync {
        if ctx.try_sync().await {
            println!(
                "{}",
                format!("  {} Synced with server", icons.success).green()
            );
        } else {
            println!("{}", format!("  {} Queued for sync", icons.queued).yellow());
        }
    }

    Ok(())
}

/// Show back-links (what references this document).
///
/// Forward references come from the local CRDT. Back-links are an
/// ephemeral server-side index, so we try the server with a 2s timeout
/// and gracefully degrade if unreachable.
pub async fn backlinks(config: &Config, doc_id: &str, no_sync: bool) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;
    let ctx = LocalContext::new(config, !no_sync)?;
    let doc_id = crate::utils::resolve_document_id(&ctx.cache, doc_id)?;

    // Forward refs from local CRDT
    let doc = ctx.load_document(&client, &doc_id).await?;
    let has_forward = !doc.references.is_empty();

    if has_forward {
        println!("{}", "Forward references:".bold());
        for r in &doc.references {
            println!(
                "  {} {} ({}) [{}]",
                r.ref_type.dimmed(),
                r.target_id.cyan(),
                r.target_type,
                "explicit".dimmed()
            );
        }
    }

    // Back-links from server (best-effort with timeout)
    let back_refs = if !no_sync {
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.get_references(&doc_id, "document"),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
    } else {
        None
    };

    let has_back = back_refs.as_ref().is_some_and(|refs| !refs.back.is_empty());

    if !has_forward && !has_back {
        println!(
            "{}",
            format!("{} No references found.", icons.info).dimmed()
        );
        return Ok(());
    }

    if let Some(refs) = back_refs
        && !refs.back.is_empty()
    {
        if has_forward {
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

/// Convert a CachedDocument to a Document for output functions.
///
/// Content, references, and custom default to empty since they are not
/// stored in the SQLite cache (full data lives in the CRDT files).
fn cached_to_document(c: CachedDocument) -> Document {
    Document {
        id: c.id,
        namespace_id: c.namespace_id,
        title: c.title,
        content: String::new(),
        created: c.created,
        modified: c.modified,
        deleted: c.deleted,
        version: c.version,
        parent_id: c.parent_id,
        slug: c.slug,
        slug_aliases: c.slug_aliases,
        labels: c.labels,
        references: Vec::new(),
        custom: serde_json::Value::Object(Default::default()),
    }
}

/// Convert a CachedNamespace to a Namespace for output functions.
fn cached_to_namespace(c: &crate::cache::CachedNamespace) -> Namespace {
    Namespace {
        id: c.id.clone(),
        name: c.name.clone(),
        description: None,
        parent_id: c.parent_id.clone(),
        labels: c.labels.clone(),
        deleted: c.deleted.clone(),
    }
}
