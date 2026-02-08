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

/// Create a new task.
pub async fn run(
    config: &Config,
    project: &str,
    title: &str,
    body: Option<String>,
    priority: &str,
    size: &str,
) -> Result<()> {
    let client = Client::new(config)?;

    let req = CreateTaskRequest {
        title: title.to_string(),
        body: body.unwrap_or_default(),
        priority: priority.to_string(),
        size: size.to_string(),
    };

    let task = client.create_task(project, &req).await?;

    println!("{}", "✓ Task created successfully!".green().bold());
    println!("  ID:       {}", task.metadata.id.to_string().cyan());
    println!("  Title:    {}", task.title);
    println!("  Priority: {}", task.metadata.priority);
    println!("  Size:     {}", task.metadata.size);
    println!(
        "\nView with: {}",
        format!("gtr show {}", task.metadata.id).dimmed()
    );

    Ok(())
}
