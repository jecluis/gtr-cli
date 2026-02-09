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

//! Project command implementation.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::models::CreateProjectRequest;
use crate::output;

/// Create a new project.
pub async fn create(config: &Config, name: &str, description: Option<String>) -> Result<()> {
    let client = Client::new(config)?;

    // Generate slug-like ID from name
    let id = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let req = CreateProjectRequest {
        id,
        name: name.to_string(),
        description,
    };

    let project = client.create_project(&req).await?;

    println!("{}", "✓ Project created successfully!".green().bold());
    println!("  ID:          {}", project.id.cyan());
    println!("  Name:        {}", project.name);
    if let Some(desc) = &project.description {
        println!("  Description: {}", desc);
    }
    println!(
        "\nCreate tasks: {}",
        format!("gtr new <title> -p {}", project.id).dimmed()
    );

    Ok(())
}

/// List all projects.
pub async fn list(config: &Config) -> Result<()> {
    let client = Client::new(config)?;
    let projects = client.list_projects().await?;

    output::print_projects(&projects);
    Ok(())
}
