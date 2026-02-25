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

//! Sync logic for bidirectional task synchronization.

use automerge::sync::SyncDoc;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::Result;
use crate::cache::{CachedNamespace, CachedProject, TaskCache};
use crate::client::Client;
use crate::config::Config;
use crate::storage::TaskStorage;

/// Sync coordinator handling local-remote synchronization.
pub struct SyncManager {
    client: Client,
    storage: TaskStorage,
    cache: TaskCache,
    client_id: String,
}

impl SyncManager {
    pub fn new(config: &Config, storage: TaskStorage, cache: TaskCache) -> Result<Self> {
        let client = Client::new(config)?;
        Ok(Self {
            client,
            storage,
            cache,
            client_id: config.client_id.clone(),
        })
    }

    /// Attempt sync with timeout. Returns true if successful.
    pub async fn try_sync(&self, timeout: Duration) -> bool {
        let sync_future = self.sync_all();
        match tokio::time::timeout(timeout, sync_future).await {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                warn!("sync failed: {e}");
                false
            }
            Err(_) => {
                warn!(timeout_ms = timeout.as_millis(), "sync timed out");
                false
            }
        }
    }

    /// Sync all pending changes (push only).
    ///
    /// This is used for automatic sync after updates. It pushes local changes
    /// and fetches the merged state, but doesn't pull all tasks to avoid
    /// unnecessary network traffic.
    pub async fn sync_all(&self) -> Result<()> {
        // Push local changes — attempt both even if one fails
        let task_result = self.push_pending().await;
        let doc_result = self.push_pending_documents().await;

        // Propagate first error so try_sync() returns false
        task_result?;
        doc_result?;

        Ok(())
    }

    /// Full bidirectional sync (push and pull).
    ///
    /// This is used for manual sync commands. It pushes local changes and
    /// then pulls updates from the server for all tasks, ensuring we have
    /// the latest state from other devices.
    pub async fn sync_full(&self) -> Result<()> {
        // Push local changes first
        self.push_pending().await?;
        self.push_pending_documents().await?;

        // Pull remote changes for all tasks
        self.pull_updates().await?;

        // Sync namespaces and links
        self.sync_namespaces().await?;
        self.sync_namespace_links().await?;

        // Pull all documents
        self.pull_all_documents().await?;

        Ok(())
    }

    /// Sync projects from server into local cache.
    async fn sync_projects(&self) -> Result<()> {
        let projects = self.client.list_projects_all(true).await?;
        let now = chrono::Utc::now().to_rfc3339();

        for project in &projects {
            let cached = CachedProject {
                id: project.id.clone(),
                name: project.name.clone(),
                parent_id: project.parent_id.clone(),
                deleted: project.deleted.clone(),
                last_synced: Some(now.clone()),
                labels: project.labels.clone(),
            };
            self.cache.upsert_project(&cached)?;
        }

        info!(count = projects.len(), "synced projects from server");
        Ok(())
    }

    /// Pull updates from server for all projects.
    async fn pull_updates(&self) -> Result<()> {
        // Sync project list first
        self.sync_projects().await?;

        // Get all projects
        let projects = self.client.list_projects().await?;

        for project in projects {
            // Get all tasks for this project
            let tasks = self
                .client
                .list_tasks(&project.id, None, None, true, true, false, false, None)
                .await?;

            for task in tasks {
                // Always pull and merge - CRDT handles conflicts automatically.
                // Pass project_id from the API response so the cache uses the
                // server's canonical UUID, even if the CRDT file still stores
                // an old string project name.
                if let Err(e) = self.pull_task(&task.id, &task.project_id).await {
                    warn!(task_id = %task.id, "failed to pull task: {e}");
                    // Continue with other tasks
                }
            }
        }

        Ok(())
    }

    /// Pull a single task from server and merge with local version.
    ///
    /// `canonical_project_id` is the server's authoritative project UUID.
    /// CRDT files may still store old string project names; we override
    /// with the canonical UUID when updating the local cache.
    async fn pull_task(&self, task_id: &str, canonical_project_id: &str) -> Result<()> {
        // Fetch CRDT bytes from server
        let remote_bytes = self.client.fetch_sync(task_id).await?;
        debug!(
            task_id,
            remote_bytes_len = remote_bytes.len(),
            "fetched remote CRDT state"
        );

        // Check if we have a local version to merge with
        let merged_bytes = match self.storage.get_task_bytes(task_id) {
            Ok(local_bytes) => {
                // We have local version - merge with remote
                debug!(
                    task_id,
                    local_bytes_len = local_bytes.len(),
                    "merging with local version"
                );
                let mut local_doc = crate::crdt::TaskDocument::load(&local_bytes)?;
                let mut remote_doc = crate::crdt::TaskDocument::load(&remote_bytes)?;

                // CRDT merge handles conflicts automatically
                local_doc.merge(&mut remote_doc)?;

                local_doc.save()
            }
            Err(_) => {
                // No local version - use remote as-is
                info!(task_id, "no local version, using remote as-is");
                remote_bytes
            }
        };

        // Save merged result to local storage
        self.storage.save_task_bytes(task_id, &merged_bytes)?;

        // Extract task data for cache, using canonical project_id from
        // server instead of whatever the CRDT file stores.
        let doc = crate::crdt::TaskDocument::load(&merged_bytes)?;
        let mut task = doc.to_task()?;
        task.project_id = canonical_project_id.to_string();

        // Update cache
        self.cache.upsert_task(&task, false)?;

        Ok(())
    }

    /// Push all locally modified tasks to server.
    async fn push_pending(&self) -> Result<()> {
        let pending_ids = self.cache.get_pending_tasks()?;
        debug!(pending_count = pending_ids.len(), "pushing pending tasks");

        let mut last_error = None;
        for task_id in pending_ids {
            if let Err(e) = self.push_task(&task_id).await {
                warn!(task_id = %task_id, "failed to push task: {e}");
                last_error = Some(e);
            }
        }

        match last_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Push a single task to server using Automerge sync protocol.
    ///
    /// The sync protocol requires multiple round trips: the client and
    /// server exchange messages until both sides converge. Each round
    /// trip may transfer heads, bloom filters, or actual changes.
    async fn push_task(&self, task_id: &str) -> Result<()> {
        // Get project_id from cache
        let summary = self
            .cache
            .get_task_summary(task_id)?
            .ok_or_else(|| crate::Error::TaskNotFound(format!("task {task_id} not in cache")))?;

        // Load local CRDT document
        let bytes = self.storage.get_task_bytes(task_id)?;
        debug!(task_id, bytes_len = bytes.len(), project_id = %summary.project_id, "pushing task");
        let mut local_doc = crate::crdt::TaskDocument::load(&bytes)?;

        // Load or create sync state for this task
        let mut sync_state = match self.cache.load_sync_state(task_id)? {
            Some(state_bytes) => automerge::sync::State::decode(&state_bytes)
                .map_err(|e| crate::Error::Storage(format!("invalid sync state: {:?}", e)))?,
            None => automerge::sync::State::new(),
        };

        // Sync loop: exchange messages until both sides converge
        const MAX_ROUNDS: usize = 10;
        let mut first_round = true;
        for _ in 0..MAX_ROUNDS {
            // Generate outgoing sync message
            let outgoing_msg = match local_doc.inner_mut().generate_sync_message(&mut sync_state) {
                Some(msg) => msg,
                None => break, // In sync, nothing more to send
            };

            let msg_bytes = outgoing_msg.encode();

            // Send to server and get response
            let result = self
                .client
                .sync_message(task_id, &msg_bytes, &self.client_id)
                .await;

            // Handle response or fall back to full-document sync for new tasks
            let response_bytes = match result {
                Ok(bytes) => bytes,
                Err(crate::Error::TaskNotFound(_)) if first_round => {
                    // Task doesn't exist on server yet — use full-document sync
                    info!(task_id, "task not on server, falling back to full-doc sync");
                    let _merged_task = self
                        .client
                        .post_sync(&summary.project_id, task_id, &bytes)
                        .await?;

                    // Fetch merged document from server and merge locally
                    let server_bytes = self.client.fetch_sync(task_id).await?;
                    let mut server_doc = crate::crdt::TaskDocument::load(&server_bytes)?;
                    local_doc.merge(&mut server_doc)?;
                    break;
                }
                Err(e) => return Err(e),
            };

            first_round = false;

            // Apply server's response message (empty means nothing to receive)
            if !response_bytes.is_empty() {
                let incoming_msg =
                    automerge::sync::Message::decode(&response_bytes).map_err(|e| {
                        crate::Error::Storage(format!("invalid sync response: {:?}", e))
                    })?;

                local_doc
                    .inner_mut()
                    .receive_sync_message(&mut sync_state, incoming_msg)
                    .map_err(|e| crate::Error::Storage(format!("sync protocol error: {:?}", e)))?;
            } else {
                break; // Server sent empty response — nothing left to exchange
            }
        }

        // Save updated document and sync state
        let updated_bytes = local_doc.save();
        debug!(
            task_id,
            updated_bytes_len = updated_bytes.len(),
            "saving merged document"
        );
        self.storage.save_task_bytes(task_id, &updated_bytes)?;

        let state_bytes = sync_state.encode();
        self.cache.save_sync_state(task_id, &state_bytes)?;

        // Update cache with merged task
        let task = local_doc.to_task()?;
        debug!(task_id, version = task.version, deadline = ?task.deadline, priority = %task.priority, "post-sync task state");
        self.cache.upsert_task(&task, false)?;
        self.cache.mark_synced(task_id)?;

        Ok(())
    }

    // -- Namespace sync --

    /// Sync namespaces from server into local cache.
    async fn sync_namespaces(&self) -> Result<()> {
        let namespaces = self.client.list_namespaces().await?;
        let now = chrono::Utc::now().to_rfc3339();

        for ns in &namespaces {
            let cached = CachedNamespace {
                id: ns.id.clone(),
                name: ns.name.clone(),
                parent_id: ns.parent_id.clone(),
                deleted: ns.deleted.clone(),
                last_synced: Some(now.clone()),
                labels: ns.labels.clone(),
            };
            self.cache.upsert_namespace(&cached)?;
        }

        info!(count = namespaces.len(), "synced namespaces from server");
        Ok(())
    }

    /// Sync namespace-project links from server.
    async fn sync_namespace_links(&self) -> Result<()> {
        let namespaces = self.cache.list_namespaces()?;

        for ns in &namespaces {
            match self.client.get_namespace_links(&ns.id).await {
                Ok(projects) => {
                    // Clear existing links and re-add from server
                    let existing = self.cache.get_linked_projects(&ns.id)?;
                    for old_pid in &existing {
                        self.cache.unlink_namespace_project(&ns.id, old_pid)?;
                    }
                    for project in &projects {
                        self.cache.link_namespace_project(&ns.id, &project.id)?;
                    }
                    debug!(namespace_id = %ns.id, links = projects.len(), "synced namespace links");
                }
                Err(e) => {
                    warn!(namespace_id = %ns.id, "failed to sync namespace links: {e}");
                }
            }
        }

        Ok(())
    }

    // -- Document sync --

    /// Push all locally modified documents to server.
    async fn push_pending_documents(&self) -> Result<()> {
        let pending_ids = self.cache.get_pending_documents()?;
        debug!(
            pending_count = pending_ids.len(),
            "pushing pending documents"
        );

        let mut last_error = None;
        for doc_id in pending_ids {
            if let Err(e) = self.push_document(&doc_id).await {
                warn!(doc_id = %doc_id, "failed to push document: {e}");
                last_error = Some(e);
            }
        }

        match last_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Push a single document to server using full-document sync.
    ///
    /// Documents use full-document sync (POST bytes, receive merged JSON)
    /// since the server doesn't have a sync-protocol endpoint for documents.
    async fn push_document(&self, doc_id: &str) -> Result<()> {
        // Load local CRDT bytes
        let bytes = self.storage.get_document_bytes(doc_id)?;
        debug!(doc_id, bytes_len = bytes.len(), "pushing document");

        // Post to server and get merged result
        let merged_doc = self.client.post_document_sync(doc_id, &bytes).await?;

        // Fetch merged CRDT state back from server
        let server_bytes = self.client.fetch_document_sync(doc_id).await?;

        // Merge server state into local
        let mut local_crdt = crate::crdt::PkmsDocument::load(&bytes)?;
        let mut server_crdt = crate::crdt::PkmsDocument::load(&server_bytes)?;
        local_crdt.merge(&mut server_crdt)?;

        // Save merged document
        let updated_bytes = local_crdt.save();
        self.storage.save_document_bytes(doc_id, &updated_bytes)?;

        // Update cache
        self.cache.upsert_document(&merged_doc, false)?;
        self.cache.mark_document_synced(doc_id)?;

        debug!(
            doc_id,
            version = merged_doc.version,
            "document pushed and merged"
        );
        Ok(())
    }

    /// Pull a single document from server and merge with local version.
    async fn pull_document(&self, doc_id: &str) -> Result<()> {
        let remote_bytes = self.client.fetch_document_sync(doc_id).await?;
        debug!(
            doc_id,
            remote_bytes_len = remote_bytes.len(),
            "fetched remote document CRDT state"
        );

        let merged_bytes = if self.storage.document_exists(doc_id) {
            let local_bytes = self.storage.get_document_bytes(doc_id)?;
            let mut local_crdt = crate::crdt::PkmsDocument::load(&local_bytes)?;
            let mut remote_crdt = crate::crdt::PkmsDocument::load(&remote_bytes)?;
            local_crdt.merge(&mut remote_crdt)?;
            local_crdt.save()
        } else {
            info!(doc_id, "no local document version, using remote as-is");
            remote_bytes
        };

        self.storage.save_document_bytes(doc_id, &merged_bytes)?;

        let crdt = crate::crdt::PkmsDocument::load(&merged_bytes)?;
        let doc = crdt.to_document()?;
        self.cache.upsert_document(&doc, false)?;

        Ok(())
    }

    /// Pull all documents from server across all namespaces.
    async fn pull_all_documents(&self) -> Result<()> {
        let namespaces = self.cache.list_namespaces()?;

        for ns in &namespaces {
            match self.client.list_documents(&ns.id, true).await {
                Ok(docs) => {
                    for doc in &docs {
                        if let Err(e) = self.pull_document(&doc.id).await {
                            warn!(doc_id = %doc.id, "failed to pull document: {e}");
                        }
                    }
                    debug!(namespace_id = %ns.id, count = docs.len(), "pulled documents for namespace");
                }
                Err(e) => {
                    warn!(namespace_id = %ns.id, "failed to list documents: {e}");
                }
            }
        }

        Ok(())
    }

    /// Get sync status summary.
    pub async fn sync_status(&self) -> Result<SyncStatus> {
        let pending_tasks = self.cache.get_pending_tasks()?.len();
        let pending_documents = self.cache.get_pending_documents()?.len();

        // Try to reach server (quick check with timeout)
        let server_reachable = self.client.health_check().await;

        Ok(SyncStatus {
            pending_push: pending_tasks + pending_documents,
            pending_tasks,
            pending_documents,
            server_reachable,
        })
    }
}

/// Sync status information.
#[derive(Debug)]
pub struct SyncStatus {
    pub pending_push: usize,
    pub pending_tasks: usize,
    pub pending_documents: usize,
    pub server_reachable: bool,
}
