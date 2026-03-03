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

//! Reference resolution: turns parsed wiki-links into concrete ReferenceRow
//! entries by looking up titles and namespaces in the local cache.

use tracing::debug;

use crate::Result;
use crate::cache::{ReferenceRow, TaskCache};
use crate::models::Reference;

use super::parser::{ParsedRef, RefTarget};

/// Build the full set of ReferenceRow entries for a document.
///
/// Combines explicit references (from the document model) with inline
/// references parsed from the content. Title-based references are
/// resolved to IDs via the cache; unresolvable ones are silently
/// skipped (they will be re-attempted on the next rebuild).
pub fn build_refs_for_document(
    cache: &TaskCache,
    source_id: &str,
    namespace_id: &str,
    explicit_refs: &[Reference],
    content: &str,
) -> Result<Vec<ReferenceRow>> {
    let mut rows = Vec::new();

    // Add explicit references
    for r in explicit_refs {
        rows.push(ReferenceRow {
            source_id: source_id.to_string(),
            source_type: "document".to_string(),
            target_id: r.target_id.clone(),
            target_type: r.target_type.clone(),
            ref_type: r.ref_type.clone(),
            origin: "explicit".to_string(),
        });
    }

    // Parse inline references from content
    let parsed = super::parser::parse_wiki_links(content);
    for pr in &parsed {
        if let Some(row) = resolve_parsed_ref(cache, source_id, namespace_id, pr)? {
            rows.push(row);
        }
    }

    Ok(rows)
}

/// Resolve a parsed reference for a document (with namespace context).
fn resolve_parsed_ref(
    cache: &TaskCache,
    source_id: &str,
    namespace_id: &str,
    parsed: &ParsedRef,
) -> Result<Option<ReferenceRow>> {
    match &parsed.target {
        RefTarget::TaskById(id) => Ok(Some(ReferenceRow {
            source_id: source_id.to_string(),
            source_type: "document".to_string(),
            target_id: id.to_string(),
            target_type: "task".to_string(),
            ref_type: parsed.ref_type.clone(),
            origin: "inline".to_string(),
        })),
        RefTarget::TaskByPrefix(prefix) => {
            match crate::utils::resolve_task_id_from_cache(cache, prefix) {
                Ok(id) => Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: id,
                    target_type: "task".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                })),
                Err(_) => {
                    debug!(prefix, "task prefix reference not resolved, skipping");
                    Ok(None)
                }
            }
        }
        RefTarget::DocumentById(id) => {
            let id_str = id.to_string();
            if id_str == source_id {
                return Ok(None);
            }
            Ok(Some(ReferenceRow {
                source_id: source_id.to_string(),
                source_type: "document".to_string(),
                target_id: id_str,
                target_type: "document".to_string(),
                ref_type: parsed.ref_type.clone(),
                origin: "inline".to_string(),
            }))
        }
        RefTarget::DocumentByTitle(title) => {
            // Look up in the same namespace first
            if let Some(doc) = cache.find_document_by_title(namespace_id, title)? {
                if doc.id == source_id {
                    return Ok(None);
                }
                return Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: doc.id,
                    target_type: "document".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                }));
            }
            // Fall back to any namespace
            if let Some(doc) = cache.find_document_by_title_any_namespace(title)? {
                if doc.id == source_id {
                    return Ok(None);
                }
                return Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: doc.id,
                    target_type: "document".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                }));
            }
            debug!(title, "wiki-link target document not found, skipping");
            Ok(None)
        }
        RefTarget::DocumentByTitleInNamespace(ns_name, title) => {
            if let Some(ns) = cache.find_namespace_by_name(ns_name)?
                && let Some(doc) = cache.find_document_by_title(&ns.id, title)?
            {
                if doc.id == source_id {
                    return Ok(None);
                }
                return Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: doc.id,
                    target_type: "document".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                }));
            }
            debug!(
                namespace = ns_name,
                title, "wiki-link target document not found in namespace, skipping"
            );
            Ok(None)
        }
        RefTarget::DocumentBySlug(slug) => {
            // 1. Namespace-scoped slug lookup
            if let Some(doc) = cache.find_document_by_slug(namespace_id, slug)? {
                if doc.id == source_id {
                    return Ok(None);
                }
                return Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: doc.id,
                    target_type: "document".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                }));
            }
            // 2. Alias check in same namespace
            if let Some(doc) = cache.find_document_by_slug_alias(namespace_id, slug)? {
                if doc.id == source_id {
                    return Ok(None);
                }
                return Ok(Some(ReferenceRow {
                    source_id: source_id.to_string(),
                    source_type: "document".to_string(),
                    target_id: doc.id,
                    target_type: "document".to_string(),
                    ref_type: parsed.ref_type.clone(),
                    origin: "inline".to_string(),
                }));
            }
            // 3. Global hex fallback
            if let Some(hex) = crate::slug::extract_hex_suffix(slug) {
                let docs = cache.find_documents_by_hex_suffix(hex)?;
                if docs.len() == 1 && docs[0].id != source_id {
                    return Ok(Some(ReferenceRow {
                        source_id: source_id.to_string(),
                        source_type: "document".to_string(),
                        target_id: docs[0].id.clone(),
                        target_type: "document".to_string(),
                        ref_type: parsed.ref_type.clone(),
                        origin: "inline".to_string(),
                    }));
                }
            }
            debug!(slug, "slug reference not resolved, skipping");
            Ok(None)
        }
        RefTarget::DocumentBySlugInNamespace(ns_name, slug) => {
            if let Some(ns) = cache.find_namespace_by_name(ns_name)? {
                // 1. Namespace-scoped slug lookup
                if let Some(doc) = cache.find_document_by_slug(&ns.id, slug)? {
                    if doc.id == source_id {
                        return Ok(None);
                    }
                    return Ok(Some(ReferenceRow {
                        source_id: source_id.to_string(),
                        source_type: "document".to_string(),
                        target_id: doc.id,
                        target_type: "document".to_string(),
                        ref_type: parsed.ref_type.clone(),
                        origin: "inline".to_string(),
                    }));
                }
                // 2. Alias check
                if let Some(doc) = cache.find_document_by_slug_alias(&ns.id, slug)? {
                    if doc.id == source_id {
                        return Ok(None);
                    }
                    return Ok(Some(ReferenceRow {
                        source_id: source_id.to_string(),
                        source_type: "document".to_string(),
                        target_id: doc.id,
                        target_type: "document".to_string(),
                        ref_type: parsed.ref_type.clone(),
                        origin: "inline".to_string(),
                    }));
                }
            }
            // 3. Global hex fallback (handles moved documents)
            if let Some(hex) = crate::slug::extract_hex_suffix(slug) {
                let docs = cache.find_documents_by_hex_suffix(hex)?;
                if docs.len() == 1 && docs[0].id != source_id {
                    return Ok(Some(ReferenceRow {
                        source_id: source_id.to_string(),
                        source_type: "document".to_string(),
                        target_id: docs[0].id.clone(),
                        target_type: "document".to_string(),
                        ref_type: parsed.ref_type.clone(),
                        origin: "inline".to_string(),
                    }));
                }
            }
            debug!(
                namespace = ns_name,
                slug, "slug reference not resolved in namespace, skipping"
            );
            Ok(None)
        }
        RefTarget::NamespaceByPath(path) => match crate::resolve::resolve_namespace(cache, path) {
            Ok(id) => Ok(Some(ReferenceRow {
                source_id: source_id.to_string(),
                source_type: "document".to_string(),
                target_id: id,
                target_type: "namespace".to_string(),
                ref_type: parsed.ref_type.clone(),
                origin: "inline".to_string(),
            })),
            Err(_) => {
                debug!(path, "namespace reference not resolved, skipping");
                Ok(None)
            }
        },
    }
}
