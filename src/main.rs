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
        /// Filter by project ID (can be specified multiple times). If specified without arguments, shows project picker.
        #[arg(short = 'P', long, num_args = 0.., value_name = "PROJECT")]
        project: Option<Vec<String>>,

        /// Filter by priority (now or later)
        #[arg(short, long, value_parser = ["now", "later"])]
        priority: Option<String>,

        /// Filter by size (XS, S, M, L, XL)
        #[arg(short, long)]
        size: Option<String>,

        /// Include done tasks alongside pending
        #[arg(long = "with-done")]
        with_done: bool,

        /// Show only done tasks
        #[arg(long)]
        done: bool,

        /// Include deleted tasks alongside pending
        #[arg(long = "with-deleted")]
        with_deleted: bool,

        /// Show only deleted tasks
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
        #[arg(long)]
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

        /// Disable progress bar (show numerical percentages)
        #[arg(long)]
        no_fancy: bool,

        /// Show extra columns (e.g. Modified)
        #[arg(short, long)]
        verbose: bool,

        /// Show only subtasks of this parent task
        #[arg(long = "for", value_name = "PARENT_ID")]
        for_task: Option<String>,

        /// With --for: include all descendants, not just direct children
        #[arg(long)]
        recursive: bool,

        /// Compact output (no row separators)
        #[arg(long)]
        compact: bool,

        /// Filter to tasks with any of these labels (OR logic, repeatable)
        #[arg(short = 'l', long = "label")]
        labels: Vec<String>,

        /// Show labels below each task title
        #[arg(short = 'L', long = "with-labels")]
        with_labels: bool,
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

        /// Disable line wrapping (preserves long URLs)
        #[arg(long)]
        no_wrap: bool,

        /// Show subtask tree with interactive picker
        #[arg(long)]
        tree: bool,
    },

    /// Create a new task
    New {
        /// Project ID (optional if default set)
        #[arg(short = 'P', long)]
        project: Option<String>,

        /// Task title (all remaining arguments; optional with --from or --bookmark)
        #[arg(num_args = 1..)]
        title: Vec<String>,

        /// Edit task body in external editor
        #[arg(short, long)]
        body: bool,

        /// Create task from URL (fetches title and description)
        #[arg(long, value_name = "URL", conflicts_with = "bookmark_url")]
        from: Option<String>,

        /// Create bookmark from URL (like --from but prefixes title with bookmark glyph)
        #[arg(
            short = 'B',
            long = "bookmark",
            value_name = "URL",
            conflicts_with = "from"
        )]
        bookmark_url: Option<String>,

        /// Priority (now or later)
        #[arg(short, long, default_value = "later", value_parser = ["now", "later"])]
        priority: String,

        /// Size (XS, S, M, L, XL)
        #[arg(short, long, default_value = "M")]
        size: String,

        /// Deadline (e.g., "tomorrow", "+3d", "2026-12-25")
        #[arg(short, long)]
        deadline: Option<String>,

        /// Initial progress percentage (0-100)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        progress: Option<u8>,

        /// Impact level (1=Catastrophic, 2=Significant, 3=Neutral, 4=Minor, 5=Negligible)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=5))]
        impact: Option<u8>,

        /// Joy level (0=dreading, 5=neutral, 10=love it)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(0..=10))]
        joy: Option<u8>,

        /// Parent task ID (creates subtask)
        #[arg(long = "for", value_name = "PARENT_ID")]
        parent: Option<String>,

        /// Add label(s) to the task (repeatable)
        #[arg(short = 'l', long = "label")]
        labels: Vec<String>,

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

        /// New deadline (value required unless --unset is set)
        #[arg(short, long, num_args = 0..=1, default_missing_value = "")]
        deadline: Option<String>,

        /// New progress percentage (0-100)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        progress: Option<u8>,

        /// New impact level (1=Catastrophic, 2=Significant, 3=Neutral, 4=Minor, 5=Negligible)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(1..=5))]
        impact: Option<u8>,

        /// New joy level (0=dreading, 5=neutral, 10=love it)
        #[arg(short, long, value_parser = clap::value_parser!(u8).range(0..=10))]
        joy: Option<u8>,

        /// Move task to a different project
        #[arg(short = 'P', long = "project")]
        project: Option<String>,

        /// Parent task ID (value required unless --unset is set)
        #[arg(long = "for", value_name = "PARENT_ID", num_args = 0..=1, default_missing_value = "")]
        parent: Option<String>,

        /// Unset fields given without values (e.g. --unset -d --for)
        #[arg(long)]
        unset: bool,

        /// Apply --project, --priority, and --deadline to all subtasks
        #[arg(short = 'R', long)]
        recursive: bool,

        /// Add label(s) to the task (repeatable)
        #[arg(short = 'l', long = "label")]
        labels: Vec<String>,

        /// Remove label(s) from the task (repeatable)
        #[arg(long = "unlabel")]
        unlabels: Vec<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Mark a task as done
    Done {
        /// Task IDs to mark as done (shows picker if omitted)
        #[arg(num_args = 0.., value_name = "TASK_ID")]
        task_ids: Vec<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Unmark a task as done (restore to pending)
    Undone {
        /// Task ID
        task_id: String,

        /// Progress percentage after restoring (default: 50)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        progress: Option<u8>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Delete a task (tombstone)
    Delete {
        /// Task ID
        task_id: String,

        /// Also delete all subtasks recursively
        #[arg(long)]
        recursive: bool,

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

    /// Set task progress percentage
    Progress {
        /// Progress value (0-100; omit when using --unset)
        #[arg(value_parser = clap::value_parser!(u8).range(0..=100), required_unless_present = "unset")]
        value: Option<u8>,

        /// Task ID (auto-selects from "doing" tasks if omitted)
        task_id: Option<String>,

        /// Clear progress tracking for the task
        #[arg(long, requires = "task_id")]
        unset: bool,

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

    /// Start working on a task (set to "doing")
    Start {
        /// Task ID (picks from pending tasks if omitted)
        task_id: Option<String>,

        /// Filter tasks by text (searches title, then body)
        #[arg(short, long)]
        filter: Option<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Stop working on a task (clear "doing" state)
    Stop {
        /// Task ID (picks from "doing" tasks if omitted)
        task_id: Option<String>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// List tasks currently in "doing" state
    Doing {
        /// Filter by project. No args = picker; omit entirely = all projects.
        #[arg(short = 'P', long, num_args = 0.., value_name = "PROJECT")]
        project: Option<Vec<String>>,
    },

    /// Suggest next tasks to work on (ordered by urgency)
    Next {
        /// Filter to specific project
        #[arg(short = 'P', long)]
        project: Option<String>,

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

        /// Include done and deleted tasks
        #[arg(long)]
        all: bool,

        /// Skip sync (search cache only)
        #[arg(long)]
        no_sync: bool,
    },

    /// Manage projects
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Manage configuration (editor, promotion thresholds)
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

    /// Set daily energy and focus levels (interactive picker if no args)
    Feels {
        /// Energy level (1=very low, 5=high)
        #[arg(value_parser = clap::value_parser!(u8).range(1..=5), requires = "focus")]
        energy: Option<u8>,

        /// Focus level (1=scattered, 5=deep)
        #[arg(value_parser = clap::value_parser!(u8).range(1..=5), requires = "energy")]
        focus: Option<u8>,

        /// Skip sync (work offline)
        #[arg(long)]
        no_sync: bool,
    },

    /// Show current status (feels, active tasks, counts)
    Status {
        /// Show label distribution grouped by project
        #[arg(short = 'L', long = "with-labels")]
        with_labels: bool,
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

        /// Parent project ID (creates a subproject)
        #[arg(long)]
        parent: Option<String>,
    },

    /// Update a project
    Update {
        /// Project ID
        project_id: String,

        /// New project description
        #[arg(short, long)]
        description: Option<String>,

        /// Move under a parent project (empty string to unparent)
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        parent: Option<String>,
    },

    /// Delete a project (soft-delete; must be empty)
    Delete {
        /// Project ID
        project_id: String,
    },

    /// Restore a deleted project
    Restore {
        /// Project ID
        project_id: String,
    },

    /// List all projects
    List {
        /// Include meta-root projects (e.g. <root>)
        #[arg(long)]
        all: bool,
    },

    /// Manage project labels
    Label {
        #[command(subcommand)]
        command: LabelCommands,
    },
}

#[derive(Subcommand, Debug)]
enum LabelCommands {
    /// List labels in a project (or all global labels with --all)
    List {
        /// Project ID
        #[arg(required_unless_present = "all")]
        project_id: Option<String>,

        /// Show all labels from <root> cascaded through every project
        #[arg(long, conflicts_with = "project_id")]
        all: bool,
    },

    /// Add labels to a project
    New {
        /// Project ID
        project_id: String,

        /// Labels to add
        #[arg(required = true)]
        labels: Vec<String>,
    },

    /// Delete a label from a project (removes from all tasks)
    Delete {
        /// Project ID
        project_id: String,

        /// Label to delete
        label: String,
    },

    /// Rename a label in a project (updates all tasks)
    Rename {
        /// Project ID
        project_id: String,

        /// Current label name
        old: String,

        /// New label name
        new: String,
    },
}

#[derive(Subcommand, Debug)]
enum SyncCommands {
    /// Manually sync now (push and pull)
    Now,

    /// Show sync status
    Status,
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

    /// Show or manage icon theme
    Icons {
        /// Set icon theme (unicode or nerd); opens picker if no value given
        #[arg(long, num_args = 0..=1, default_missing_value = "")]
        set: Option<String>,

        /// Unset icon theme (revert to default)
        #[arg(long)]
        unset: bool,
    },

    /// Manage promotion thresholds
    Promotion {
        #[command(subcommand)]
        command: PromotionCommands,
    },
}

#[derive(Subcommand, Debug)]
enum PromotionCommands {
    /// Show current promotion thresholds
    Show {
        /// Show project-specific configuration
        #[arg(short = 'P', long)]
        project: Option<String>,
    },

    /// Edit promotion thresholds (opens editor with JSON)
    Set {
        /// Set for project instead of user
        #[arg(short = 'P', long)]
        project: Option<String>,

        /// Read thresholds from file instead of opening editor
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Reset all overrides to defaults
    Reset {
        /// Reset project configuration instead of user
        #[arg(short = 'P', long)]
        project: Option<String>,
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

    // Initialize file-based logging
    let log_dir = config.cache_dir.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    gtr::logging::init(&log_dir, &config.log_level);

    // Override config with CLI arguments
    let mut config = config.with_server(cli.server).with_token(cli.token);

    // Execute command
    match cli.command {
        Commands::List {
            project,
            priority,
            size,
            with_done,
            done,
            with_deleted,
            deleted,
            all,
            due_soon,
            overdue,
            limit,
            reversed,
            no_sync,
            absolute,
            no_fancy,
            verbose,
            for_task,
            recursive,
            compact,
            labels,
            with_labels,
        } => {
            gtr::commands::list::tasks(
                &config,
                project,
                priority,
                size,
                with_done,
                done,
                with_deleted,
                deleted,
                all,
                due_soon,
                overdue,
                limit,
                reversed,
                no_sync,
                absolute,
                !no_fancy,
                verbose,
                for_task,
                recursive,
                compact,
                labels,
                with_labels,
            )
            .await
        }
        Commands::Show {
            task_id,
            no_sync,
            no_format,
            no_wrap,
            tree,
        } => gtr::commands::show::run(&config, &task_id, no_sync, no_format, no_wrap, tree).await,
        Commands::New {
            project,
            title,
            body,
            from,
            bookmark_url,
            priority,
            size,
            deadline,
            progress,
            impact,
            joy,
            parent,
            labels,
            no_sync,
        } => {
            let url = from.or(bookmark_url.clone());
            let is_bookmark = bookmark_url.is_some();

            // Title is required unless --from or --bookmark is provided
            if title.is_empty() && url.is_none() {
                eprintln!("Error: task title is required (or use --from / --bookmark)");
                std::process::exit(1);
            }

            let title_str = if title.is_empty() {
                None
            } else {
                Some(title.join(" "))
            };
            gtr::commands::create::run(
                &config,
                project,
                title_str,
                body,
                &priority,
                &size,
                deadline,
                progress,
                impact,
                joy,
                parent,
                labels,
                no_sync,
                url,
                is_bookmark,
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
            progress,
            impact,
            joy,
            project,
            parent,
            unset,
            recursive,
            labels,
            unlabels,
            no_sync,
        } => {
            gtr::commands::update::run(
                &config, &task_id, title, body, priority, size, deadline, progress, impact, joy,
                project, parent, unset, recursive, labels, unlabels, no_sync,
            )
            .await
        }
        Commands::Done { task_ids, no_sync } => {
            gtr::commands::done::run(&config, task_ids, no_sync).await
        }
        Commands::Undone {
            task_id,
            progress,
            no_sync,
        } => gtr::commands::undone::run(&config, &task_id, progress, no_sync).await,
        Commands::Delete {
            task_id,
            recursive,
            no_sync,
        } => gtr::commands::delete::run(&config, &task_id, recursive, no_sync).await,
        Commands::Restore { task_id, no_sync } => {
            gtr::commands::restore::run(&config, &task_id, no_sync).await
        }
        Commands::Progress {
            value,
            task_id,
            unset,
            no_sync,
        } => {
            if unset {
                gtr::commands::progress::unset(&config, task_id, no_sync).await
            } else {
                gtr::commands::progress::run(&config, value.unwrap(), task_id, no_sync).await
            }
        }
        Commands::Now { task_id, no_sync } => {
            gtr::commands::now::run(&config, &task_id, no_sync).await
        }
        Commands::Later { task_id, no_sync } => {
            gtr::commands::later::run(&config, &task_id, no_sync).await
        }
        Commands::Start {
            task_id,
            filter,
            no_sync,
        } => gtr::commands::start::run(&config, task_id, filter, no_sync).await,
        Commands::Stop { task_id, no_sync } => {
            gtr::commands::stop::run(&config, task_id, no_sync).await
        }
        Commands::Doing { project } => gtr::commands::doing::run(&config, project).await,
        Commands::Next { project, no_sync } => {
            gtr::commands::next::run(&config, project, no_sync).await
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
            all,
            no_sync,
        } => gtr::commands::search::run(&config, &query, project, limit, all, no_sync).await,
        Commands::Project { command } => match command {
            ProjectCommands::Create {
                name,
                description,
                parent,
            } => gtr::commands::project::create(&config, &name, description, parent).await,
            ProjectCommands::Update {
                project_id,
                description,
                parent,
            } => gtr::commands::project::update(&config, &project_id, description, parent).await,
            ProjectCommands::Delete { project_id } => {
                gtr::commands::project::delete(&config, &project_id).await
            }
            ProjectCommands::Restore { project_id } => {
                gtr::commands::project::restore(&config, &project_id).await
            }
            ProjectCommands::List { all } => gtr::commands::project::list(&config, all).await,
            ProjectCommands::Label { command } => match command {
                LabelCommands::List { project_id, all } => {
                    if all {
                        gtr::commands::project::label_list_all(&config).await
                    } else {
                        gtr::commands::project::label_list(&config, project_id.as_deref().unwrap())
                            .await
                    }
                }
                LabelCommands::New { project_id, labels } => {
                    gtr::commands::project::label_new(&config, &project_id, &labels).await
                }
                LabelCommands::Delete { project_id, label } => {
                    gtr::commands::project::label_delete(&config, &project_id, &label).await
                }
                LabelCommands::Rename {
                    project_id,
                    old,
                    new,
                } => gtr::commands::project::label_rename(&config, &project_id, &old, &new).await,
            },
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
            ConfigCommands::Icons { set, unset } => {
                if unset {
                    gtr::commands::config::unset_icons(&mut config)
                } else if let Some(value) = set {
                    gtr::commands::config::set_icons(&mut config, value)
                } else {
                    gtr::commands::config::show_icons(&config)
                }
            }
            ConfigCommands::Promotion { command } => match command {
                PromotionCommands::Show { project } => {
                    gtr::commands::promotion::show(&config, project).await
                }
                PromotionCommands::Set { project, file } => {
                    gtr::commands::promotion::set(&config, project, file).await
                }
                PromotionCommands::Reset { project } => {
                    gtr::commands::promotion::reset(&config, project).await
                }
            },
        },
        Commands::Sync { command } => match command {
            SyncCommands::Now => gtr::commands::sync::now(&config).await,
            SyncCommands::Status => gtr::commands::sync::status(&config).await,
        },
        Commands::Feels {
            energy,
            focus,
            no_sync,
        } => gtr::commands::feels::run(&config, energy, focus, no_sync).await,
        Commands::Status { with_labels } => gtr::commands::status::run(&config, with_labels).await,
        Commands::Init { .. } => unreachable!(),
        Commands::Version => unreachable!(),
    }
}
