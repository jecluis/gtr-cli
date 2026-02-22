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

//! HTTP client for communicating with the server.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Client as HttpClient, StatusCode};

use crate::config::Config;
use crate::models::*;
use crate::{Error, Result};

/// Percent-encode a value for use as a URL path segment.
fn encode_path(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

/// API client for the Getting Things Rusty server.
pub struct Client {
    http: HttpClient,
    base_url: String,
    auth_token: String,
}

impl Client {
    /// Header name for client version identification.
    const VERSION_HEADER: &'static str = "x-gtr-client-version";

    /// Create a new client from configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let mut default_headers = reqwest::header::HeaderMap::new();
        default_headers.insert(
            Self::VERSION_HEADER,
            reqwest::header::HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
        );

        let http = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .default_headers(default_headers)
            .build()?;

        Ok(Client {
            http,
            base_url: config.server_url.trim_end_matches('/').to_string(),
            auth_token: config.auth_token.clone(),
        })
    }

    /// Get server version information (no auth required).
    pub async fn get_version(&self) -> Result<VersionInfo> {
        let url = format!("{}/api/version", self.base_url);
        let resp = self.http.get(&url).send().await?;

        if resp.status().is_success() {
            Ok(resp.json::<VersionInfo>().await?)
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Quick health check to see if server is reachable.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/version", self.base_url);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.http.get(&url).send(),
        )
        .await;

        matches!(result, Ok(Ok(resp)) if resp.status().is_success())
    }

    /// Post CRDT bytes to server for merging (sync).
    pub async fn post_sync(&self, _project_id: &str, task_id: &str, bytes: &[u8]) -> Result<Task> {
        let url = format!("{}/api/sync/{}", self.base_url, encode_path(task_id));
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(resp.json::<Task>().await?)
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Fetch CRDT bytes from server for a task.
    pub async fn fetch_sync(&self, task_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/api/sync/{}", self.base_url, encode_path(task_id));
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Sync with server using Automerge sync protocol.
    ///
    /// Sends a sync::Message and receives the server's sync::Message response.
    pub async fn sync_message(
        &self,
        task_id: &str,
        message_bytes: &[u8],
        client_id: &str,
    ) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/sync/protocol/{}",
            self.base_url,
            encode_path(task_id)
        );

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("x-client-id", client_id)
            .header("content-type", "application/octet-stream")
            .body(message_bytes.to_vec())
            .send()
            .await?;

        if resp.status().is_success() {
            let response_bytes = resp.bytes().await?;
            Ok(response_bytes.to_vec())
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// List all projects.
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let url = format!("{}/api/projects", self.base_url);
        self.get(&url).await
    }

    /// List projects, optionally including meta-root projects.
    pub async fn list_projects_all(&self, include_meta: bool) -> Result<Vec<Project>> {
        let url = if include_meta {
            format!("{}/api/projects?include_meta=true", self.base_url)
        } else {
            format!("{}/api/projects", self.base_url)
        };
        self.get(&url).await
    }

    /// Get a specific project.
    pub async fn get_project(&self, id: &str) -> Result<Project> {
        let url = format!("{}/api/projects/{}", self.base_url, encode_path(id));
        self.get(&url).await
    }

    /// Create a new project.
    pub async fn create_project(&self, req: &CreateProjectRequest) -> Result<Project> {
        let url = format!("{}/api/projects", self.base_url);
        self.post(&url, req).await
    }

    /// Update a project.
    pub async fn update_project(&self, id: &str, req: &UpdateProjectRequest) -> Result<Project> {
        let url = format!("{}/api/projects/{}", self.base_url, encode_path(id));
        self.put(&url, req).await
    }

    /// Delete (soft-delete) a project.
    pub async fn delete_project(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/projects/{}", self.base_url, encode_path(id));
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Restore a soft-deleted project.
    pub async fn restore_project(&self, id: &str) -> Result<Project> {
        let url = format!("{}/api/projects/{}/restore", self.base_url, encode_path(id));
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// List tasks in a project.
    #[allow(clippy::too_many_arguments)]
    pub async fn list_tasks(
        &self,
        project_id: &str,
        priority: Option<&str>,
        size: Option<&str>,
        include_done: bool,
        include_deleted: bool,
        due_soon: bool,
        overdue: bool,
        limit: Option<u32>,
    ) -> Result<Vec<Task>> {
        let mut url = format!(
            "{}/api/projects/{}/tasks",
            self.base_url,
            encode_path(project_id)
        );
        let mut params = Vec::new();

        if let Some(p) = priority {
            params.push(format!("priority={}", p));
        }
        if let Some(s) = size {
            params.push(format!("size={}", s));
        }
        if include_done {
            params.push("include_done=true".to_string());
        }
        if include_deleted {
            params.push("include_deleted=true".to_string());
        }
        if due_soon {
            params.push("due_soon=true".to_string());
        }
        if overdue {
            params.push("overdue=true".to_string());
        }
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        self.get(&url).await
    }

    /// Move a task to a different project.
    pub async fn move_task(&self, task_id: &str, target_project_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}/move", self.base_url, encode_path(task_id));
        let req = crate::models::MoveTaskRequest {
            target_project_id: target_project_id.to_string(),
        };
        self.post(&url, &req).await
    }

    /// Get a specific task.
    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}", self.base_url, encode_path(task_id));
        self.get(&url).await
    }

    /// Create a new task.
    pub async fn create_task(&self, project_id: &str, req: &CreateTaskRequest) -> Result<Task> {
        let url = format!(
            "{}/api/projects/{}/tasks",
            self.base_url,
            encode_path(project_id)
        );
        self.post(&url, req).await
    }

    /// Update a task.
    pub async fn update_task(&self, task_id: &str, req: &UpdateTaskRequest) -> Result<Task> {
        let url = format!("{}/api/tasks/{}", self.base_url, encode_path(task_id));
        self.put(&url, req).await
    }

    /// Search tasks.
    pub async fn search_tasks(
        &self,
        query: &str,
        project: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Task>> {
        let mut url = format!("{}/api/search?q={}", self.base_url, encode_path(query));

        if let Some(p) = project {
            url.push_str(&format!("&project={}", encode_path(p)));
        }
        if let Some(l) = limit {
            url.push_str(&format!("&limit={}", l));
        }

        self.get(&url).await
    }

    /// Generic GET request.
    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Generic POST request.
    async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Generic PUT request.
    async fn put<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .http
            .put(url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Handle HTTP response and deserialize or return error.
    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T> {
        let status = resp.status();

        if status.is_success() {
            let body = resp.json::<T>().await?;
            Ok(body)
        } else {
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Get user configuration.
    pub async fn get_user_config(&self) -> Result<ConfigResponse> {
        let url = format!("{}/api/config", self.base_url);
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            let config: ConfigResponse = serde_json::from_str(&text)
                .map_err(|e| Error::Server(format!("Invalid JSON response: {}", e)))?;
            Ok(config)
        } else {
            Err(self.error_from_response(status, &text))
        }
    }

    /// Update user configuration.
    pub async fn update_user_config(&self, req: &ConfigUpdateRequest) -> Result<ConfigResponse> {
        let url = format!("{}/api/config", self.base_url);
        let response = self
            .http
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(req)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            let config: ConfigResponse = serde_json::from_str(&text)
                .map_err(|e| Error::Server(format!("Invalid JSON response: {}", e)))?;
            Ok(config)
        } else {
            Err(self.error_from_response(status, &text))
        }
    }

    /// Reset user promotion config to defaults.
    pub async fn reset_user_config(&self) -> Result<()> {
        let url = format!("{}/api/config/promotion", self.base_url);
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Get project configuration.
    pub async fn get_project_config(&self, project_id: &str) -> Result<ConfigResponse> {
        let url = format!(
            "{}/api/projects/{}/config",
            self.base_url,
            encode_path(project_id)
        );
        let response = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            let config: ConfigResponse = serde_json::from_str(&text)
                .map_err(|e| Error::Server(format!("Invalid JSON response: {}", e)))?;
            Ok(config)
        } else {
            Err(self.error_from_response(status, &text))
        }
    }

    /// Update project configuration.
    pub async fn update_project_config(
        &self,
        project_id: &str,
        req: &ConfigUpdateRequest,
    ) -> Result<ConfigResponse> {
        let url = format!(
            "{}/api/projects/{}/config",
            self.base_url,
            encode_path(project_id)
        );
        let response = self
            .http
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(req)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if status.is_success() {
            let config: ConfigResponse = serde_json::from_str(&text)
                .map_err(|e| Error::Server(format!("Invalid JSON response: {}", e)))?;
            Ok(config)
        } else {
            Err(self.error_from_response(status, &text))
        }
    }

    /// Reset project promotion config.
    pub async fn reset_project_config(&self, project_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/projects/{}/config/promotion",
            self.base_url,
            encode_path(project_id)
        );
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Add labels to a project's registry.
    pub async fn create_project_labels(
        &self,
        project_id: &str,
        labels: &[String],
    ) -> Result<Project> {
        let url = format!(
            "{}/api/projects/{}/labels",
            self.base_url,
            encode_path(project_id)
        );
        let body = serde_json::json!({ "labels": labels });
        self.post(&url, &body).await
    }

    /// Delete a label from a project's registry (removes from all tasks too).
    pub async fn delete_project_label(
        &self,
        project_id: &str,
        label: &str,
    ) -> Result<LabelMutationResponse> {
        let url = format!(
            "{}/api/projects/{}/labels/{}",
            self.base_url,
            encode_path(project_id),
            encode_path(label)
        );
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        self.handle_response(response).await
    }

    /// Rename a label in a project's registry (updates all tasks too).
    pub async fn rename_project_label(
        &self,
        project_id: &str,
        old: &str,
        new_name: &str,
    ) -> Result<LabelMutationResponse> {
        let url = format!(
            "{}/api/projects/{}/labels/{}",
            self.base_url,
            encode_path(project_id),
            encode_path(old)
        );
        let body = serde_json::json!({ "name": new_name });
        self.put(&url, &body).await
    }

    /// Post a feels (energy/focus) entry to the server.
    pub async fn post_feels(&self, energy: u8, focus: u8, utc_offset: &str) -> Result<()> {
        let url = format!("{}/api/feels", self.base_url);
        let body = serde_json::json!({
            "energy": energy,
            "focus": focus,
            "utc_offset": utc_offset,
        });
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    // -- Namespace API --

    /// List all namespaces.
    pub async fn list_namespaces(&self) -> Result<Vec<Namespace>> {
        let url = format!("{}/api/namespaces", self.base_url);
        self.get(&url).await
    }

    /// Get a specific namespace.
    pub async fn get_namespace(&self, id: &str) -> Result<Namespace> {
        let url = format!("{}/api/namespaces/{}", self.base_url, encode_path(id));
        self.get(&url).await
    }

    /// Create a new namespace.
    pub async fn create_namespace(&self, req: &CreateNamespaceRequest) -> Result<Namespace> {
        let url = format!("{}/api/namespaces", self.base_url);
        self.post(&url, req).await
    }

    /// Update a namespace.
    pub async fn update_namespace(
        &self,
        id: &str,
        req: &UpdateNamespaceRequest,
    ) -> Result<Namespace> {
        let url = format!("{}/api/namespaces/{}", self.base_url, encode_path(id));
        self.put(&url, req).await
    }

    /// Delete (soft-delete) a namespace.
    pub async fn delete_namespace(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/namespaces/{}", self.base_url, encode_path(id));
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Restore a soft-deleted namespace.
    pub async fn restore_namespace(&self, id: &str) -> Result<Namespace> {
        let url = format!(
            "{}/api/namespaces/{}/restore",
            self.base_url,
            encode_path(id)
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        self.handle_response(resp).await
    }

    // -- Namespace-project links --

    /// Link a project to a namespace.
    pub async fn link_namespace_project(&self, namespace_id: &str, project_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/namespaces/{}/links/{}",
            self.base_url,
            encode_path(namespace_id),
            encode_path(project_id)
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Unlink a project from a namespace.
    pub async fn unlink_namespace_project(
        &self,
        namespace_id: &str,
        project_id: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/api/namespaces/{}/links/{}",
            self.base_url,
            encode_path(namespace_id),
            encode_path(project_id)
        );
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Get projects linked to a namespace.
    pub async fn get_namespace_links(&self, namespace_id: &str) -> Result<Vec<Project>> {
        let url = format!(
            "{}/api/namespaces/{}/links",
            self.base_url,
            encode_path(namespace_id)
        );
        self.get(&url).await
    }

    // -- Document API --

    /// List documents in a namespace.
    pub async fn list_documents(
        &self,
        namespace_id: &str,
        include_deleted: bool,
    ) -> Result<Vec<Document>> {
        let mut url = format!(
            "{}/api/namespaces/{}/documents",
            self.base_url,
            encode_path(namespace_id)
        );
        if include_deleted {
            url.push_str("?include_deleted=true");
        }
        self.get(&url).await
    }

    /// Get a specific document.
    pub async fn get_document(&self, id: &str) -> Result<Document> {
        let url = format!("{}/api/documents/{}", self.base_url, encode_path(id));
        self.get(&url).await
    }

    /// Create a new document.
    pub async fn create_document(
        &self,
        namespace_id: &str,
        req: &CreateDocumentRequest,
    ) -> Result<Document> {
        let url = format!(
            "{}/api/namespaces/{}/documents",
            self.base_url,
            encode_path(namespace_id)
        );
        self.post(&url, req).await
    }

    /// Update a document.
    pub async fn update_document(&self, id: &str, req: &UpdateDocumentRequest) -> Result<Document> {
        let url = format!("{}/api/documents/{}", self.base_url, encode_path(id));
        self.put(&url, req).await
    }

    /// Delete (soft-delete) a document.
    pub async fn delete_document(&self, id: &str) -> Result<()> {
        let url = format!("{}/api/documents/{}", self.base_url, encode_path(id));
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Restore a soft-deleted document.
    pub async fn restore_document(&self, id: &str) -> Result<Document> {
        let url = format!(
            "{}/api/documents/{}/restore",
            self.base_url,
            encode_path(id)
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        self.handle_response(resp).await
    }

    /// Move a document to a different namespace.
    pub async fn move_document(&self, doc_id: &str, target_namespace_id: &str) -> Result<Document> {
        let url = format!(
            "{}/api/documents/{}/move",
            self.base_url,
            encode_path(doc_id)
        );
        let req = MoveDocumentRequest {
            target_namespace_id: target_namespace_id.to_string(),
        };
        self.post(&url, &req).await
    }

    // -- Document sync --

    /// Post document CRDT bytes to server for merging.
    pub async fn post_document_sync(&self, doc_id: &str, bytes: &[u8]) -> Result<Document> {
        let url = format!(
            "{}/api/sync/documents/{}",
            self.base_url,
            encode_path(doc_id)
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(resp.json::<Document>().await?)
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Fetch document CRDT bytes from server.
    pub async fn fetch_document_sync(&self, doc_id: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/sync/documents/{}",
            self.base_url,
            encode_path(doc_id)
        );
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            let status = resp.status();
            let text = resp.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    // -- Reference API --

    /// Get references for an entity (forward + back).
    pub async fn get_references(&self, id: &str, entity_type: &str) -> Result<EntityReferences> {
        let url = format!(
            "{}/api/references/{}?type={}",
            self.base_url,
            encode_path(id),
            encode_path(entity_type)
        );
        self.get(&url).await
    }

    /// Add a reference from a task to another entity.
    pub async fn add_task_reference(
        &self,
        task_id: &str,
        req: &AddReferenceRequest,
    ) -> Result<ReferenceResponse> {
        let url = format!(
            "{}/api/tasks/{}/references",
            self.base_url,
            encode_path(task_id)
        );
        self.post(&url, req).await
    }

    /// Remove a reference from a task.
    pub async fn remove_task_reference(&self, task_id: &str, target_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/tasks/{}/references/{}",
            self.base_url,
            encode_path(task_id),
            encode_path(target_id)
        );
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Add a reference from a document to another entity.
    pub async fn add_document_reference(
        &self,
        doc_id: &str,
        req: &AddReferenceRequest,
    ) -> Result<ReferenceResponse> {
        let url = format!(
            "{}/api/documents/{}/references",
            self.base_url,
            encode_path(doc_id)
        );
        self.post(&url, req).await
    }

    /// Remove a reference from a document.
    pub async fn remove_document_reference(&self, doc_id: &str, target_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/documents/{}/references/{}",
            self.base_url,
            encode_path(doc_id),
            encode_path(target_id)
        );
        let response = self
            .http
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = response.text().await?;
            Err(self.error_from_response(status, &text))
        }
    }

    /// Convert HTTP error response to Error.
    fn error_from_response(&self, status: StatusCode, body: &str) -> Error {
        match status {
            StatusCode::NOT_FOUND => {
                let body_lower = body.to_lowercase();
                if body_lower.contains("task") {
                    Error::TaskNotFound(body.to_string())
                } else if body_lower.contains("project") {
                    Error::ProjectNotFound(body.to_string())
                } else {
                    Error::Server(format!("404: {}", body))
                }
            }
            StatusCode::BAD_REQUEST => Error::InvalidInput(body.to_string()),
            StatusCode::UNAUTHORIZED => Error::Server("Authentication failed".to_string()),
            _ => Error::Server(format!("HTTP {}: {}", status, body)),
        }
    }
}
