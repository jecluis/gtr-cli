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

//! Task reference command implementations.

use colored::Colorize;

use crate::Result;
use crate::client::Client;
use crate::config::Config;
use crate::icons::Icons;
use crate::models::AddReferenceRequest;

/// Add a reference from a task to another entity.
pub async fn add_ref(
    config: &Config,
    task_id: &str,
    target: &str,
    target_type: &str,
    ref_type: &str,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    let req = AddReferenceRequest {
        target_id: target.to_string(),
        target_type: target_type.to_string(),
        ref_type: ref_type.to_string(),
    };

    client.add_task_reference(task_id, &req).await?;

    println!(
        "{}",
        format!(
            "{} Reference added: {} --[{}]--> {} ({})",
            icons.success, task_id, ref_type, target, target_type
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Remove a reference from a task.
pub async fn remove_ref(
    config: &Config,
    task_id: &str,
    target: &str,
    _no_sync: bool,
) -> Result<()> {
    let icons = Icons::new(config.effective_icon_theme());
    let client = Client::new(config)?;

    client.remove_task_reference(task_id, target).await?;

    println!(
        "{}",
        format!(
            "{} Reference removed: {} -/-> {}",
            icons.success, task_id, target
        )
        .green()
        .bold()
    );

    Ok(())
}
