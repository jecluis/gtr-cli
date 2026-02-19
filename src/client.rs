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
    /// Create a new client from configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let http = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
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
