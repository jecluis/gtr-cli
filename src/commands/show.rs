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

//! Show command implementation.

use crate::Result;
use crate::client::Client;
use crate::config::Config;

/// Show a specific task.
pub async fn run(config: &Config, task_id: &str) -> Result<()> {
    let client = Client::new(config)?;
    let _task = client.get_task(task_id).await?;

    // TODO: Pretty markdown output
    println!("Show task {} - to be implemented", task_id);
    Ok(())
}
