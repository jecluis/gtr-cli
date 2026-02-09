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

use reqwest::{Client as HttpClient, StatusCode};

use crate::config::Config;
use crate::models::*;
use crate::{Error, Result};

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

    /// List all projects.
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let url = format!("{}/api/projects", self.base_url);
        self.get(&url).await
    }

    /// Get a specific project.
    pub async fn get_project(&self, id: &str) -> Result<Project> {
        let url = format!("{}/api/projects/{}", self.base_url, id);
        self.get(&url).await
    }

    /// Create a new project.
    pub async fn create_project(&self, req: &CreateProjectRequest) -> Result<Project> {
        let url = format!("{}/api/projects", self.base_url);
        self.post(&url, req).await
    }

    /// List tasks in a project.
    pub async fn list_tasks(
        &self,
        project_id: &str,
        priority: Option<&str>,
        size: Option<&str>,
        include_done: bool,
        include_deleted: bool,
        limit: Option<u32>,
    ) -> Result<Vec<Task>> {
        let mut url = format!("{}/api/projects/{}/tasks", self.base_url, project_id);
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
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        self.get(&url).await
    }

    /// Get a specific task.
    pub async fn get_task(&self, task_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}", self.base_url, task_id);
        self.get(&url).await
    }

    /// Create a new task.
    pub async fn create_task(&self, project_id: &str, req: &CreateTaskRequest) -> Result<Task> {
        let url = format!("{}/api/projects/{}/tasks", self.base_url, project_id);
        self.post(&url, req).await
    }

    /// Update a task.
    pub async fn update_task(&self, task_id: &str, req: &UpdateTaskRequest) -> Result<Task> {
        let url = format!("{}/api/tasks/{}", self.base_url, task_id);
        self.put(&url, req).await
    }

    /// Mark a task as done.
    pub async fn mark_done(&self, task_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}/done", self.base_url, task_id);
        self.post(&url, &()).await
    }

    /// Unmark a task as done (restore to pending).
    pub async fn mark_undone(&self, task_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}/done", self.base_url, task_id);
        self.delete_with_response(&url).await
    }

    /// Delete a task (tombstone).
    pub async fn delete_task(&self, task_id: &str) -> Result<()> {
        let url = format!("{}/api/tasks/{}", self.base_url, task_id);
        self.delete(&url).await
    }

    /// Restore a deleted task.
    pub async fn restore_task(&self, task_id: &str) -> Result<Task> {
        let url = format!("{}/api/tasks/{}/restore", self.base_url, task_id);
        self.post(&url, &()).await
    }

    /// Search tasks.
    pub async fn search_tasks(
        &self,
        query: &str,
        project: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<Task>> {
        let mut url = format!("{}/api/search?q={}", self.base_url, query);

        if let Some(p) = project {
            url.push_str(&format!("&project={}", p));
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

    /// Generic DELETE request (no response body).
    async fn delete(&self, url: &str) -> Result<()> {
        let resp = self
            .http
            .delete(url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
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

    /// Generic DELETE request with response body.
    async fn delete_with_response<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .http
            .delete(url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
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

    /// Convert HTTP error response to Error.
    fn error_from_response(&self, status: StatusCode, body: &str) -> Error {
        match status {
            StatusCode::NOT_FOUND => {
                if body.contains("task") {
                    Error::TaskNotFound(body.to_string())
                } else if body.contains("project") {
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
