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

//! Create command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::models::CreateTaskRequest;
use crate::utils;

/// Create a new task.
pub async fn run(
    config: &Config,
    project: Option<String>,
    title: &str,
    body: Option<String>,
    priority: &str,
    size: &str,
    deadline: Option<String>,
) -> Result<()> {
    let client = Client::new(config)?;
    let project_id = utils::resolve_project(&client, project).await?;

    let req = CreateTaskRequest {
        title: title.to_string(),
        body: body.unwrap_or_default(),
        priority: priority.to_string(),
        size: size.to_string(),
        deadline,
    };

    let task = client.create_task(&project_id, &req).await?;

    println!("{}", "✓ Task created successfully!".green().bold());
    println!("  ID:       {}", task.id.cyan());
    println!("  Title:    {}", task.title);
    println!("  Priority: {}", task.priority);
    println!("  Size:     {}", task.size);
    println!("\nView with: {}", format!("gtr show {}", task.id).dimmed());

    Ok(())
}
