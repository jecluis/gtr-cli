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

//! Automerge CRDT document wrapper for PKMS documents.

use automerge::{Automerge, ObjType, ROOT, ReadDoc, transaction::Transactable};

use crate::models::Document;
use crate::{Error, Result};

/// A PKMS document represented as an Automerge CRDT document.
pub struct PkmsDocument {
    doc: Automerge,
}

impl PkmsDocument {
    /// Get mutable reference to inner Automerge document.
    pub fn inner_mut(&mut self) -> &mut Automerge {
        &mut self.doc
    }

    /// Create a new CRDT document from a Document model.
    pub fn new(document: &Document) -> Result<Self> {
        let mut doc = Automerge::new();

        // Pre-serialize complex fields
        let labels_json = serde_json::to_string(&document.labels)
            .map_err(|e| Error::Storage(format!("labels serialization failed: {e}")))?;
        let references_json = serde_json::to_string(&document.references)
            .map_err(|e| Error::Storage(format!("references serialization failed: {e}")))?;
        let custom_json = serde_json::to_string(&document.custom)
            .map_err(|e| Error::Storage(format!("custom serialization failed: {e}")))?;

        doc.transact::<_, _, automerge::AutomergeError>(|tx| {
            // Create base metadata map
            let base = tx.put_object(ROOT, "base", ObjType::Map)?;

            tx.put(&base, "id", document.id.as_str())?;
            tx.put(&base, "title", document.title.as_str())?;
            tx.put(&base, "created", document.created.as_str())?;
            tx.put(&base, "modified", document.modified.as_str())?;
            tx.put(&base, "version", document.version as i64)?;

            if let Some(ref deleted) = document.deleted {
                tx.put(&base, "deleted", deleted.as_str())?;
            }

            if let Some(ref parent_id) = document.parent_id {
                tx.put(&base, "parent_id", parent_id.as_str())?;
            }

            // Labels, references, custom (stored as JSON strings)
            tx.put(&base, "labels", labels_json.as_str())?;
            tx.put(&base, "references", references_json.as_str())?;
            tx.put(&base, "custom", custom_json.as_str())?;

            // Content and namespace_id at root level
            tx.put(ROOT, "content", document.content.as_str())?;
            tx.put(ROOT, "namespace_id", document.namespace_id.as_str())?;

            // Slug fields at root level
            tx.put(ROOT, "slug", document.slug.as_str())?;
            let slug_aliases_json =
                serde_json::to_string(&document.slug_aliases).unwrap_or_else(|_| "[]".to_string());
            tx.put(ROOT, "slug_aliases", slug_aliases_json.as_str())?;

            Ok(())
        })
        .map_err(|e| Error::Storage(format!("failed to create PKMS document: {e:?}")))?;

        Ok(Self { doc })
    }

    /// Load a document from bytes.
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let doc = Automerge::load(bytes)
            .map_err(|e| Error::Storage(format!("failed to load PKMS document: {e:?}")))?;
        Ok(Self { doc })
    }

    /// Save document to bytes.
    pub fn save(&self) -> Vec<u8> {
        self.doc.save()
    }

    /// Convert document to Document model.
    pub fn to_document(&self) -> Result<Document> {
        // Try "base" key first (new format), fall back to "metadata"
        let base_id = match self.doc.get(ROOT, "base") {
            Ok(Some((automerge::Value::Object(automerge::ObjType::Map), obj_id))) => obj_id,
            _ => {
                // Fallback: try "metadata" key for compatibility
                match self.doc.get(ROOT, "metadata") {
                    Ok(Some((automerge::Value::Object(automerge::ObjType::Map), obj_id))) => obj_id,
                    _ => return Err(Error::Storage("missing base/metadata map".to_string())),
                }
            }
        };

        let id = self.get_str(&base_id, "id")?;
        let title = self.get_str(&base_id, "title")?;
        let namespace_id = self.get_str(&ROOT, "namespace_id")?;
        let content = self.try_get_str(&ROOT, "content")?.unwrap_or_default();
        let created = self.get_str(&base_id, "created")?;
        let modified = self.get_str(&base_id, "modified")?;
        let version = self.get_i64(&base_id, "version")? as u64;

        let deleted = self.try_get_str(&base_id, "deleted")?;
        let parent_id = self.try_get_str(&base_id, "parent_id")?;

        // Parse labels
        let labels: Vec<String> = self
            .try_get_str(&base_id, "labels")?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| Error::Storage(format!("invalid labels JSON: {e}")))?
            .unwrap_or_default();

        // Parse references
        let references: Vec<crate::models::Reference> = self
            .try_get_str(&base_id, "references")?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| Error::Storage(format!("invalid references JSON: {e}")))?
            .unwrap_or_default();

        // Parse custom
        let custom: serde_json::Value = self
            .try_get_str(&base_id, "custom")?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| Error::Storage(format!("invalid custom JSON: {e}")))?
            .unwrap_or(serde_json::Value::Object(Default::default()));

        Ok(Document {
            id,
            namespace_id,
            title,
            content,
            created,
            modified,
            deleted,
            version,
            parent_id,
            slug: self.try_get_str(&ROOT, "slug")?.unwrap_or_default(),
            slug_aliases: self
                .try_get_str(&ROOT, "slug_aliases")?
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| Error::Storage(format!("invalid slug_aliases JSON: {e}")))?
                .unwrap_or_default(),
            labels,
            references,
            custom,
        })
    }

    /// Update document with new data (only changed fields).
    pub fn update_document(&mut self, document: &Document) -> Result<()> {
        let current = self.to_document()?;

        // Pre-serialize changed complex fields
        let labels_json = if document.labels != current.labels {
            Some(
                serde_json::to_string(&document.labels)
                    .map_err(|e| Error::Storage(format!("labels serialization failed: {e}")))?,
            )
        } else {
            None
        };
        let references_json = if document.references != current.references {
            Some(
                serde_json::to_string(&document.references)
                    .map_err(|e| Error::Storage(format!("references serialization failed: {e}")))?,
            )
        } else {
            None
        };
        let custom_json = if document.custom != current.custom {
            Some(
                serde_json::to_string(&document.custom)
                    .map_err(|e| Error::Storage(format!("custom serialization failed: {e}")))?,
            )
        } else {
            None
        };

        self.doc
            .transact::<_, _, automerge::AutomergeError>(|tx| {
                let base = match tx.get(ROOT, "base").ok().flatten() {
                    Some((automerge::Value::Object(_), id)) => id,
                    _ => {
                        // Try metadata fallback
                        match tx.get(ROOT, "metadata").ok().flatten() {
                            Some((automerge::Value::Object(_), id)) => id,
                            _ => tx.put_object(ROOT, "base", ObjType::Map)?,
                        }
                    }
                };

                if document.modified != current.modified {
                    tx.put(&base, "modified", document.modified.as_str())?;
                }
                if document.version != current.version {
                    tx.put(&base, "version", document.version as i64)?;
                }

                if document.deleted != current.deleted {
                    if let Some(ref deleted) = document.deleted {
                        tx.put(&base, "deleted", deleted.as_str())?;
                    } else {
                        let _ = tx.delete(&base, "deleted");
                    }
                }

                if document.parent_id != current.parent_id {
                    if let Some(ref pid) = document.parent_id {
                        tx.put(&base, "parent_id", pid.as_str())?;
                    } else {
                        let _ = tx.delete(&base, "parent_id");
                    }
                }

                if let Some(ref lj) = labels_json {
                    tx.put::<_, _, &str>(&base, "labels", lj)?;
                }
                if let Some(ref rj) = references_json {
                    tx.put::<_, _, &str>(&base, "references", rj)?;
                }
                if let Some(ref cj) = custom_json {
                    tx.put::<_, _, &str>(&base, "custom", cj)?;
                }

                if document.title != current.title {
                    tx.put(&base, "title", document.title.as_str())?;
                }
                if document.content != current.content {
                    tx.put(ROOT, "content", document.content.as_str())?;
                }
                if document.namespace_id != current.namespace_id {
                    tx.put(ROOT, "namespace_id", document.namespace_id.as_str())?;
                }

                if document.slug != current.slug {
                    tx.put(ROOT, "slug", document.slug.as_str())?;
                }
                if document.slug_aliases != current.slug_aliases {
                    let aliases_json = serde_json::to_string(&document.slug_aliases)
                        .unwrap_or_else(|_| "[]".to_string());
                    tx.put(ROOT, "slug_aliases", aliases_json.as_str())?;
                }

                Ok(())
            })
            .map_err(|e| Error::Storage(format!("failed to update PKMS document: {e:?}")))?;

        Ok(())
    }

    /// Merge another document into this one.
    pub fn merge(&mut self, other: &mut PkmsDocument) -> Result<()> {
        self.doc
            .merge(&mut other.doc)
            .map_err(|e| Error::Storage(format!("PKMS document merge failed: {e:?}")))?;
        Ok(())
    }

    /// Get the namespace_id from the document without full deserialization.
    pub fn get_namespace_id(&self) -> Result<String> {
        self.get_str(&ROOT, "namespace_id")
    }

    // -- Helper methods --

    fn get_str(&self, obj: &automerge::ObjId, key: &str) -> Result<String> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => s
                .to_str()
                .map(|s| s.to_string())
                .ok_or_else(|| Error::Storage(format!("field {key} is not a string"))),
            _ => Err(Error::Storage(format!("missing field: {key}"))),
        }
    }

    fn try_get_str(&self, obj: &automerge::ObjId, key: &str) -> Result<Option<String>> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => Ok(s.to_str().map(|s| s.to_string())),
            Ok(None) | Ok(Some(_)) => Ok(None),
            Err(e) => Err(Error::Storage(format!("error reading {key}: {e:?}"))),
        }
    }

    fn get_i64(&self, obj: &automerge::ObjId, key: &str) -> Result<i64> {
        match self.doc.get(obj, key) {
            Ok(Some((automerge::Value::Scalar(s), _))) => s
                .to_i64()
                .ok_or_else(|| Error::Storage(format!("invalid i64 field: {key}"))),
            _ => Err(Error::Storage(format!("missing field: {key}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use automerge::ReadDoc;

    fn sample_document() -> Document {
        Document {
            id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            namespace_id: "11111111-2222-3333-4444-555555555555".to_string(),
            title: "Test Document".to_string(),
            content: "Some content.".to_string(),
            created: "2026-01-01T00:00:00Z".to_string(),
            modified: "2026-01-01T00:00:00Z".to_string(),
            deleted: None,
            version: 1,
            parent_id: None,
            slug: "test-document-aaaaaaaa".to_string(),
            slug_aliases: vec![],
            labels: vec![],
            references: vec![],
            custom: serde_json::Value::Object(Default::default()),
        }
    }

    /// Verify the CRDT field layout matches the server's convention.
    ///
    /// The server (via base_helpers) puts title inside the base map and
    /// namespace_id at ROOT.  The CLI must match so that CRDT merges
    /// between server and CLI produce a document both sides can read.
    #[test]
    fn crdt_layout_matches_server() {
        let doc = sample_document();
        let crdt = PkmsDocument::new(&doc).unwrap();

        let base_id = match crdt.doc.get(ROOT, "base").unwrap() {
            Some((automerge::Value::Object(automerge::ObjType::Map), id)) => id,
            _ => panic!("base map missing"),
        };

        // title must be inside base
        assert!(
            crdt.doc.get(&base_id, "title").unwrap().is_some(),
            "title should be inside base map"
        );
        assert!(
            crdt.doc.get(ROOT, "title").unwrap().is_none(),
            "title should NOT be at ROOT"
        );

        // namespace_id must be at ROOT
        assert!(
            crdt.doc.get(ROOT, "namespace_id").unwrap().is_some(),
            "namespace_id should be at ROOT"
        );
        assert!(
            crdt.doc.get(&base_id, "namespace_id").unwrap().is_none(),
            "namespace_id should NOT be inside base"
        );
    }

    #[test]
    fn roundtrip() {
        let doc = sample_document();
        let crdt = PkmsDocument::new(&doc).unwrap();
        let bytes = crdt.save();
        let loaded = PkmsDocument::load(&bytes).unwrap();
        let recovered = loaded.to_document().unwrap();

        assert_eq!(recovered.id, doc.id);
        assert_eq!(recovered.title, doc.title);
        assert_eq!(recovered.namespace_id, doc.namespace_id);
        assert_eq!(recovered.content, doc.content);
        assert_eq!(recovered.slug, doc.slug);
    }
}
