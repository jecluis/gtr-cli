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

//! File-based logging for the CLI.
//!
//! Logs are written to `{cache_dir}/logs/` with daily rotation. Terminal
//! output is reserved for user-facing CLI output (`println!`/`colored`),
//! so tracing only writes to files.
//!
//! Log level priority: `GTR_LOG` env var > config `log_level` > `"info"`.

use std::path::Path;

use tracing_appender::rolling;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

/// Initialize file-based logging.
///
/// Writes logs to `{log_dir}/gtr.log.YYYY-MM-DD` with daily rotation.
/// The `GTR_LOG` environment variable takes precedence over the config
/// `log_level` for filter directives.
pub fn init(log_dir: &Path, config_level: &str) {
    let filter = if let Ok(gtf_log) = std::env::var("GTR_LOG") {
        EnvFilter::new(gtf_log)
    } else {
        EnvFilter::new(config_level)
    };

    let file_appender = rolling::daily(log_dir, "gtr.log");

    fmt()
        .with_env_filter(filter)
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .init();
}
