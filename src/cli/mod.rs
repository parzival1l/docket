use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod init;

#[derive(Parser)]
#[command(name = "docket", version, about = "Agent-shaped task tracker with TDD execution harness")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .docket/ in the current repo
    Init,
    /// Add a task
    Add {
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
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
    },
    /// List tasks
    Ls {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show a single task
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Tasks ready to pick up (open with all deps done)
    Ready {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Tasks blocked by unmet deps (debug view)
    Blocked {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Set status (open | in_progress | done — or any string)
    Status { id: String, state: String },
    /// Mark task done
    Done { id: String },
    /// Remove a task
    Rm { id: String },
    /// Print a prompt template to stdout
    Prompt {
        /// One of: tdd-pursuit, create-task, commit, pr
        name: String,
    },
    /// Start a task: assemble its prompt and print to stdout, mark in_progress
    Start {
        id: String,
        /// Open a new tmux window in the current session and deliver the prompt to it
        #[arg(long)]
        tmux: bool,
    },
    /// Group operations
    Group {
        #[command(subcommand)]
        action: GroupCommand,
    },
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
    match cli.command {
        Command::Init => init::run(),
        Command::Add {
            title,
            body,
            acceptance,
            deps,
            priority,
            group,
        } => crate::cmd_add(title, body, acceptance, deps, priority, group),
        Command::Ls { status, group, json } => crate::cmd_ls(status, group, json),
        Command::Show { id, json } => crate::cmd_show(id, json),
        Command::Ready { group, json } => crate::cmd_ready(group, json),
        Command::Blocked { group, json } => crate::cmd_blocked(group, json),
        Command::Status { id, state } => crate::cmd_status(id, state),
        Command::Done { id } => crate::cmd_done(id),
        Command::Rm { id } => crate::cmd_rm(id),
        Command::Prompt { name } => crate::cmd_prompt(name),
        Command::Start { id, tmux } => crate::cmd_start(id, tmux),
        Command::Group { action } => match action {
            GroupCommand::New {
                name,
                branch,
                description,
            } => crate::cmd_group_new(name, branch, description),
            GroupCommand::Ls { json } => crate::cmd_group_ls(json),
            GroupCommand::Show { name, json } => crate::cmd_group_show(name, json),
            GroupCommand::Close { name } => crate::cmd_group_close(name),
        },
    }
}
