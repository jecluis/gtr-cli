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

use clap::{Parser, Subcommand};

use gtr::Result;
use gtr::config::Config;

/// Getting Things Rusty - ADHD-friendly task tracker CLI
#[derive(Parser, Debug)]
#[command(name = "gtr")]
#[command(version, about, long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, env = "GTR_CONFIG")]
    config: Option<String>,

    /// Server URL (overrides config)
    #[arg(long, env = "GTR_SERVER_URL")]
    server: Option<String>,

    /// Auth token (overrides config)
    #[arg(long, env = "GTR_AUTH_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List tasks
    List {
        /// Filter by project ID
        #[arg(short = 'P', long)]
        project: Option<String>,

        /// Filter by priority (for-now, not-for-now)
        #[arg(long)]
        priority: Option<String>,

        /// Filter by size (XS, S, M, L, XL)
        #[arg(long)]
        size: Option<String>,

        /// Maximum number of results
        #[arg(short, long)]
        limit: Option<u32>,
    },

    /// Show a specific task
    Show {
        /// Task ID
        task_id: String,
    },

    /// Create a new task
    New {
        /// Project ID
        #[arg(short = 'P', long)]
        project: String,

        /// Task title (all remaining arguments)
        #[arg(num_args = 1.., required = true)]
        title: Vec<String>,

        /// Task body/description
        #[arg(short, long)]
        body: Option<String>,

        /// Priority (for-now or not-for-now)
        #[arg(short, long, default_value = "not-for-now")]
        priority: String,

        /// Size (XS, S, M, L, XL)
        #[arg(short, long, default_value = "M")]
        size: String,
    },

    /// Update an existing task
    Update {
        /// Task ID
        task_id: String,

        /// New title
        #[arg(short, long)]
        title: Option<String>,

        /// New body
        #[arg(short, long)]
        body: Option<String>,

        /// New priority
        #[arg(long)]
        priority: Option<String>,

        /// New size
        #[arg(long)]
        size: Option<String>,
    },

    /// Delete a task
    Delete {
        /// Task ID
        task_id: String,
    },

    /// Search tasks
    Search {
        /// Search query
        query: String,

        /// Filter by project
        #[arg(short, long)]
        project: Option<String>,

        /// Maximum number of results
        #[arg(short, long)]
        limit: Option<u32>,
    },

    /// Manage projects
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Initialize configuration
    Init {
        /// Server URL
        #[arg(short, long)]
        server: String,

        /// Authentication token
        #[arg(short, long)]
        token: String,
    },
}

#[derive(Subcommand, Debug)]
enum ProjectCommands {
    /// Create a new project
    Create {
        /// Project name
        name: String,

        /// Project description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// List all projects
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle init command separately (doesn't need existing config)
    if let Commands::Init { server, token } = cli.command {
        return gtr::commands::init::run(&server, &token);
    }

    // Load configuration
    let config = Config::load(cli.config.as_deref())?;

    // Override config with CLI arguments
    let config = config.with_server(cli.server).with_token(cli.token);

    // Execute command
    match cli.command {
        Commands::List {
            project,
            priority,
            size,
            limit,
        } => gtr::commands::list::tasks(&config, project, priority, size, limit).await,
        Commands::Show { task_id } => gtr::commands::show::run(&config, &task_id).await,
        Commands::New {
            project,
            title,
            body,
            priority,
            size,
        } => {
            let title_str = title.join(" ");
            gtr::commands::create::run(&config, &project, &title_str, body, &priority, &size).await
        }
        Commands::Update {
            task_id,
            title,
            body,
            priority,
            size,
        } => gtr::commands::update::run(&config, &task_id, title, body, priority, size).await,
        Commands::Delete { task_id } => gtr::commands::delete::run(&config, &task_id).await,
        Commands::Search {
            query,
            project,
            limit,
        } => gtr::commands::search::run(&config, &query, project, limit).await,
        Commands::Project { command } => match command {
            ProjectCommands::Create { name, description } => {
                gtr::commands::project::create(&config, &name, description).await
            }
            ProjectCommands::List => gtr::commands::project::list(&config).await,
        },
        Commands::Init { .. } => unreachable!(),
    }
}
