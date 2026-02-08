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

//! List command implementation.

use crate::client::Client;
use crate::config::Config;
use crate::output;
use crate::{Error, Result};

/// List tasks.
pub async fn tasks(
    config: &Config,
    project: Option<String>,
    priority: Option<String>,
    size: Option<String>,
    limit: Option<u32>,
) -> Result<()> {
    let client = Client::new(config)?;

    // Project ID is required for listing tasks
    let project_id = project.ok_or_else(|| {
        Error::InvalidInput(
            "project ID required. Use --project <id> or list all projects with --projects"
                .to_string(),
        )
    })?;

    let tasks = client
        .list_tasks(&project_id, priority.as_deref(), size.as_deref(), limit)
        .await?;

    output::print_tasks(&tasks);
    Ok(())
}
