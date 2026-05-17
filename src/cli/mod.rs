use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::model::{fmt_id, kind_short, Task};

pub mod add;
pub mod backlog;
pub mod blocked;
pub mod group;
pub mod init;
pub mod ls;
pub mod prompt;
pub mod ready;
pub mod rm;
pub mod show;
pub mod start;
pub mod status;
pub mod tui;
pub mod update;

pub(crate) fn print_task_row(t: &Task) {
    let group_str = t
        .group
        .as_deref()
        .map(|g| format!(" [{}]", g))
        .unwrap_or_default();
    println!(
        "{:<6} {:<12} {:<4} p{} {}{}",
        fmt_id(t.id),
        t.status,
        kind_short(&t.kind),
        t.priority,
        t.title,
        group_str
    );
}

#[derive(Parser)]
#[command(name = "docket", version, about = "Agent-shaped task tracker with TDD execution harness")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .docket/ in the current repo
    Init,
    /// Add a task
    Add {
        title: String,
        #[arg(long, allow_hyphen_values = true)]
        body: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        acceptance: Option<String>,
        /// Comma- or space-separated task IDs (e.g. "T-3,T-5" or "3 5")
        #[arg(long)]
        deps: Option<String>,
        /// 0..4, lower = higher priority (default 2)
        #[arg(long)]
        priority: Option<i32>,
        /// Group name; created lazily if it doesn't exist
        #[arg(long)]
        group: Option<String>,
        /// One of: bug, feature, chore, docs, spike (default: feature)
        #[arg(long, value_parser = ["bug", "feature", "chore", "docs", "spike"])]
        kind: Option<String>,
        /// Park the task in the backlog instead of `open` — captures the idea without
        /// cluttering `ls`/`ready`. Surface it later with `docket backlog`, pull it
        /// onto the active board with `docket promote T-N`.
        #[arg(long)]
        backlog: bool,
    },
    /// List active tasks. Hides `backlog` by default; pass `--status backlog`
    /// (or use `docket backlog`) to see parked tasks.
    Ls {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        group: Option<String>,
        /// Filter by kind: bug, feature, chore, docs, spike
        #[arg(long, value_parser = ["bug", "feature", "chore", "docs", "spike"])]
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a single task
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Tasks ready to pick up (status = `open` with all deps `done`).
    /// Backlog tasks are excluded — promote them first with `docket promote T-N`.
    Ready {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Tasks blocked by unmet deps (debug view). Backlog tasks are excluded.
    Blocked {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List parked tasks (status = `backlog`). The backlog is a separate "later" list
    /// that does NOT appear in `ls`/`ready`/`blocked`. Use `docket promote T-N` to
    /// move a task from here onto the active board.
    Backlog {
        #[arg(long)]
        group: Option<String>,
        /// Filter by kind: bug, feature, chore, docs, spike
        #[arg(long, value_parser = ["bug", "feature", "chore", "docs", "spike"])]
        kind: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Move a task from `backlog` to `open` (pull it onto the active board).
    /// Errors if the task isn't currently in backlog.
    Promote { id: String },
    /// Set status (backlog | open | in_progress | done — or any string).
    /// Lifecycle: backlog → open → in_progress → done.
    Status { id: String, state: String },
    /// Mark task done
    Done { id: String },
    /// Remove a task
    Rm { id: String },
    /// Update fields on an existing task (at least one field flag required)
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        body: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        acceptance: Option<String>,
        /// Comma- or space-separated task IDs (e.g. "T-3,T-5" or "3 5"); replaces existing deps
        #[arg(long)]
        deps: Option<String>,
        /// 0..4, lower = higher priority
        #[arg(long)]
        priority: Option<i32>,
        /// Group name; created lazily if it doesn't exist
        #[arg(long)]
        group: Option<String>,
        /// One of: bug, feature, chore, docs, spike
        #[arg(long, value_parser = ["bug", "feature", "chore", "docs", "spike"])]
        kind: Option<String>,
    },
    /// Print a prompt template to stdout
    Prompt {
        /// One of: tdd-pursuit, create-task, commit, pr
        name: String,
    },
    /// Start a task: assemble its prompt and print to stdout, mark in_progress
    Start {
        id: String,
        /// Create a fresh tmux session running claude with the prompt piped in,
        /// then attach (switch-client inside tmux, or spawn a terminal window
        /// via $DOCKET_TERMINAL_CMD when outside tmux).
        #[arg(long)]
        tmux: bool,
    },
    /// Group operations
    Group {
        #[command(subcommand)]
        action: GroupCommand,
    },
    /// Open the interactive TUI
    Tui,
}

#[derive(Subcommand)]
pub enum GroupCommand {
    /// Create a new group
    New {
        name: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// List groups with task counts
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Show a group with its tasks
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Mark a group closed
    Close { name: String },
}

pub fn dispatch(cli: Cli) -> Result<()> {
    let command = match cli.command {
        Some(c) => c,
        None => return tui::run(),
    };
    match command {
        Command::Init => init::run(),
        Command::Add {
            title,
            body,
            acceptance,
            deps,
            priority,
            group,
            kind,
            backlog,
        } => add::run(title, body, acceptance, deps, priority, group, kind, backlog),
        Command::Ls {
            status,
            group,
            kind,
            json,
        } => ls::run(status, group, kind, json),
        Command::Show { id, json } => show::run(id, json),
        Command::Ready { group, json } => ready::run(group, json),
        Command::Blocked { group, json } => blocked::run(group, json),
        Command::Backlog { group, kind, json } => backlog::run(group, kind, json),
        Command::Promote { id } => status::promote(id),
        Command::Status { id, state } => status::run(id, state),
        Command::Done { id } => status::done(id),
        Command::Rm { id } => rm::run(id),
        Command::Update {
            id,
            title,
            body,
            acceptance,
            deps,
            priority,
            group,
            kind,
        } => update::run(id, title, body, acceptance, deps, priority, group, kind),
        Command::Prompt { name } => prompt::run(name),
        Command::Start { id, tmux } => start::run(
            id,
            if tmux {
                start::TmuxDelivery::Auto
            } else {
                start::TmuxDelivery::Off
            },
        ),
        Command::Group { action } => match action {
            GroupCommand::New {
                name,
                branch,
                description,
            } => group::new(name, branch, description),
            GroupCommand::Ls { json } => group::ls(json),
            GroupCommand::Show { name, json } => group::show(name, json),
            GroupCommand::Close { name } => group::close(name),
        },
        Command::Tui => tui::run(),
    }
}
