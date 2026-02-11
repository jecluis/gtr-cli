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
        /// Filter by project ID (can be specified multiple times)
        #[arg(short = 'P', long)]
        project: Vec<String>,

        /// List tasks from all projects
        #[arg(long)]
        all_projects: bool,

        /// Filter by priority (now or later)
        #[arg(short, long, value_parser = ["now", "later"])]
        priority: Option<String>,

        /// Filter by size (XS, S, M, L, XL)
        #[arg(short, long)]
        size: Option<String>,

        /// Include done tasks
        #[arg(long)]
        done: bool,

        /// Include deleted tasks
        #[arg(long)]
        deleted: bool,

        /// Include all tasks (done and deleted)
        #[arg(long)]
        all: bool,

        /// Show only tasks due within 48 hours
        #[arg(long = "due-soon")]
        due_soon: bool,

        /// Show only overdue tasks
        #[arg(long)]
        overdue: bool,

        /// Maximum number of results
        #[arg(short, long)]
        limit: Option<u32>,

        /// Reverse the order of non-doing tasks
        #[arg(short, long)]
        reversed: bool,

        /// Skip sync (use cache only)
        #[arg(long)]
        no_sync: bool,

        /// Show absolute dates instead of relative (e.g., "2026-02-15" instead of "in 4 days")
        #[arg(long)]
        absolute: bool,
    },

    /// Show a specific task
    Show {
        /// Task ID
        task_id: String,

        /// Skip sync refresh (use cached only)
        #[arg(long)]
        no_sync: bool,

        /// Disable markdown formatting (plain text)
        #[arg(long)]
        no_format: bool,
    },

    /// Create a new task
    New {
        /// Project ID (optional if default set)
        #[arg(short = 'P', long)]
        project: Option<String>,

        /// Task title (all remaining arguments)
        #[arg(num_args = 1.., required = true)]
        title: Vec<String>,

        /// Edit task body in external editor
        #[arg(short, long)]
        body: bool,

        /// Priority (now or later)
        #[arg(short, long, default_value = "later", value_parser = ["now", "later"])]
        priority: String,

        /// Size (XS, S, M, L, XL)
        #[arg(short, long, default_value = "M")]
        size: String,

        /// Deadline (e.g., "tomorrow", "+3d", "2026-12-25")
        #[arg(short, long)]
        deadline: Option<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Update an existing task
    Update {
        /// Task ID
        task_id: String,

        /// New title
        #[arg(short, long)]
        title: Option<String>,

        /// Edit task body in external editor
        #[arg(short, long)]
        body: bool,

        /// New priority (now or later)
        #[arg(short, long, value_parser = ["now", "later"])]
        priority: Option<String>,

        /// New size
        #[arg(short, long)]
        size: Option<String>,

        /// New deadline (use "none" to clear)
        #[arg(short, long)]
        deadline: Option<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Mark a task as done
    Done {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Unmark a task as done (restore to pending)
    Undone {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Delete a task (tombstone)
    Delete {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Restore a deleted task
    Restore {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Set task priority to "now"
    Now {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Set task priority to "later"
    Later {
        /// Task ID
        task_id: String,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Display task change log
    Log {
        /// Task ID
        task_id: String,

        /// Show only work state changes
        #[arg(long)]
        work: bool,

        /// Show only state changes (priority, size, etc.)
        #[arg(long)]
        state: bool,

        /// Skip sync (use cached log)
        #[arg(long)]
        no_sync: bool,
    },

    /// Search tasks
    Search {
        /// Search query
        query: String,

        /// Filter by project
        #[arg(short = 'P', long)]
        project: Option<String>,

        /// Maximum number of results
        #[arg(short, long)]
        limit: Option<u32>,

        /// Skip sync (search cache only)
        #[arg(long)]
        no_sync: bool,
    },

    /// Manage projects
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Manage deadline promotion thresholds
    Deadline {
        #[command(subcommand)]
        command: DeadlineCommands,
    },

    /// Manage general configuration (editor, etc.)
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
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

    /// Show version information (CLI and server)
    Version,

    /// Synchronize with server
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
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

    /// Update a project
    Update {
        /// Project ID
        project_id: String,

        /// New project description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// List all projects
    List,
}

#[derive(Subcommand, Debug)]
enum SyncCommands {
    /// Manually sync now (push and pull)
    Now,

    /// Show sync status
    Status,
}

#[derive(Subcommand, Debug)]
enum DeadlineCommands {
    /// Show current deadline promotion thresholds
    Show {
        /// Show project-specific configuration
        #[arg(short = 'P', long)]
        project: Option<String>,
    },

    /// Set deadline threshold for a specific task size
    Set {
        /// Task size (XS, S, M, L, XL)
        size: String,

        /// Threshold duration (e.g., "24h", "3d", "1w")
        duration: String,

        /// Set for project instead of user
        #[arg(short = 'P', long)]
        project: Option<String>,
    },

    /// Remove deadline threshold override for a specific size
    Unset {
        /// Task size (XS, S, M, L, XL)
        size: String,

        /// Remove from project instead of user
        #[arg(short = 'P', long)]
        project: Option<String>,
    },

    /// Reset all overrides to defaults
    Reset {
        /// Reset project configuration instead of user
        #[arg(short = 'P', long)]
        project: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Show or manage editor configuration
    Editor {
        /// Set editor command (with optional args)
        #[arg(long)]
        set: Option<String>,

        /// Unset editor (fall back to $EDITOR or default)
        #[arg(long)]
        unset: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Handle init command separately (doesn't need existing config)
    if let Commands::Init { server, token } = cli.command {
        return gtr::commands::init::run(&server, &token);
    }

    // Handle version command (may need config for server version)
    if let Commands::Version = cli.command {
        let config = Config::load(cli.config.as_deref()).ok();
        return gtr::commands::version::run(config.as_ref()).await;
    }

    // Load configuration
    let config = Config::load(cli.config.as_deref())?;

    // Override config with CLI arguments
    let mut config = config.with_server(cli.server).with_token(cli.token);

    // Execute command
    match cli.command {
        Commands::List {
            project,
            all_projects,
            priority,
            size,
            done,
            deleted,
            all,
            due_soon,
            overdue,
            limit,
            reversed,
            no_sync,
            absolute,
        } => {
            let include_done = all || done;
            let include_deleted = all || deleted;
            gtr::commands::list::tasks(
                &config,
                project,
                all_projects,
                priority,
                size,
                include_done,
                include_deleted,
                due_soon,
                overdue,
                limit,
                reversed,
                no_sync,
                absolute,
            )
            .await
        }
        Commands::Show {
            task_id,
            no_sync,
            no_format,
        } => gtr::commands::show::run(&config, &task_id, no_sync, no_format).await,
        Commands::New {
            project,
            title,
            body,
            priority,
            size,
            deadline,
            no_sync,
        } => {
            let title_str = title.join(" ");
            gtr::commands::create::run(
                &config, project, &title_str, body, &priority, &size, deadline, no_sync,
            )
            .await
        }
        Commands::Update {
            task_id,
            title,
            body,
            priority,
            size,
            deadline,
            no_sync,
        } => {
            gtr::commands::update::run(
                &config, &task_id, title, body, priority, size, deadline, no_sync,
            )
            .await
        }
        Commands::Done { task_id, no_sync } => {
            gtr::commands::done::run(&config, &task_id, no_sync).await
        }
        Commands::Undone { task_id, no_sync } => {
            gtr::commands::undone::run(&config, &task_id, no_sync).await
        }
        Commands::Delete { task_id, no_sync } => {
            gtr::commands::delete::run(&config, &task_id, no_sync).await
        }
        Commands::Restore { task_id, no_sync } => {
            gtr::commands::restore::run(&config, &task_id, no_sync).await
        }
        Commands::Now { task_id, no_sync } => {
            gtr::commands::now::run(&config, &task_id, no_sync).await
        }
        Commands::Later { task_id, no_sync } => {
            gtr::commands::later::run(&config, &task_id, no_sync).await
        }
        Commands::Log {
            task_id,
            work,
            state,
            no_sync,
        } => gtr::commands::log::run(&config, &task_id, work, state, no_sync).await,
        Commands::Search {
            query,
            project,
            limit,
            no_sync,
        } => gtr::commands::search::run(&config, &query, project, limit, no_sync).await,
        Commands::Project { command } => match command {
            ProjectCommands::Create { name, description } => {
                gtr::commands::project::create(&config, &name, description).await
            }
            ProjectCommands::Update {
                project_id,
                description,
            } => gtr::commands::project::update(&config, &project_id, description).await,
            ProjectCommands::List => gtr::commands::project::list(&config).await,
        },
        Commands::Deadline { command } => match command {
            DeadlineCommands::Show { project } => {
                gtr::commands::deadline::show(&config, project).await
            }
            DeadlineCommands::Set {
                size,
                duration,
                project,
            } => gtr::commands::deadline::set(&config, size, duration, project).await,
            DeadlineCommands::Unset { size, project } => {
                gtr::commands::deadline::unset(&config, size, project).await
            }
            DeadlineCommands::Reset { project } => {
                gtr::commands::deadline::reset(&config, project).await
            }
        },
        Commands::Config { command } => match command {
            ConfigCommands::Editor { set, unset } => {
                if unset {
                    gtr::commands::config::unset_editor(&mut config)
                } else if let Some(editor) = set {
                    gtr::commands::config::set_editor(&mut config, editor.clone())
                } else {
                    gtr::commands::config::show_editor(&config)
                }
            }
        },
        Commands::Sync { command } => match command {
            SyncCommands::Now => gtr::commands::sync::now(&config).await,
            SyncCommands::Status => gtr::commands::sync::status(&config).await,
        },
        Commands::Init { .. } => unreachable!(),
        Commands::Version => unreachable!(),
    }
}
