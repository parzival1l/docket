use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

mod db;
mod model;
mod prompts;

use db::{
    find_repo_root, get_or_create_group, init_schema, load_all_groups, load_all_tasks,
    load_group_by_name, open_db,
};
use model::{fmt_id, now, parse_deps, parse_id, Group, Task};
use prompts::{assemble_prompt, PROMPT_COMMIT, PROMPT_CREATE_TASK, PROMPT_PR, PROMPT_TDD};

#[derive(Parser)]
#[command(name = "docket", version, about = "Agent-shaped task tracker with TDD execution harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
enum GroupCommand {
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

fn print_task_row(t: &Task) {
    let group_str = t
        .group
        .as_deref()
        .map(|g| format!(" [{}]", g))
        .unwrap_or_default();
    println!(
        "{:<6} {:<12} p{} {}{}",
        fmt_id(t.id),
        t.status,
        t.priority,
        t.title,
        group_str
    );
}

// === commands ===

fn cmd_init() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd).ok_or_else(|| anyhow!("not in a git repo"))?;
    let dir = root.join(".docket");
    let db = dir.join("db.sqlite");

    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        println!("created {}", dir.display());
    }

    let conn = Connection::open(&db)?;
    init_schema(&conn)?;
    println!("initialized {}", db.display());

    let gi = root.join(".gitignore");
    let line = ".docket/";
    let needs_append = if gi.exists() {
        let c = fs::read_to_string(&gi)?;
        !c.lines().any(|l| l.trim() == line)
    } else {
        true
    };
    if needs_append {
        let mut content = if gi.exists() {
            fs::read_to_string(&gi)?
        } else {
            String::new()
        };
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(line);
        content.push('\n');
        fs::write(&gi, content)?;
        println!("added `.docket/` to {}", gi.display());
    } else {
        println!(".gitignore already excludes .docket/");
    }
    Ok(())
}

fn cmd_add(
    title: String,
    body: Option<String>,
    acceptance: Option<String>,
    deps: Option<String>,
    priority: Option<i32>,
    group: Option<String>,
) -> Result<()> {
    let conn = open_db()?;
    let group_id = match group {
        Some(name) => Some(get_or_create_group(&conn, &name)?),
        None => None,
    };
    let deps_json = parse_deps(deps)?;
    let priority = priority.unwrap_or(2);
    let id = db::insert_task(
        &conn,
        db::NewTask {
            title: &title,
            body: body.as_deref(),
            acceptance: acceptance.as_deref(),
            deps_json: deps_json.as_deref(),
            priority,
            group_id,
        },
    )?;
    println!("{} added", fmt_id(id));
    Ok(())
}

fn cmd_ls(status: Option<String>, group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let filtered: Vec<&Task> = all
        .iter()
        .filter(|t| status.as_deref().map_or(true, |s| t.status == s))
        .filter(|t| group.as_deref().map_or(true, |g| t.group.as_deref() == Some(g)))
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&filtered)?);
    } else if filtered.is_empty() {
        println!("(no tasks)");
    } else {
        for t in filtered {
            print_task_row(t);
        }
    }
    Ok(())
}

fn cmd_show(id: String, json: bool) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let t = all
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow!("{} not found", fmt_id(id)))?;
    if json {
        println!("{}", serde_json::to_string_pretty(t)?);
    } else {
        let dep_states: Vec<String> = t
            .deps
            .iter()
            .map(|d| match all.iter().find(|x| x.id == *d) {
                Some(dep) => format!("{} ({})", fmt_id(*d), dep.status),
                None => format!("{} (missing)", fmt_id(*d)),
            })
            .collect();
        println!(
            "{}  {}  [{}]  p{}",
            fmt_id(t.id),
            t.title,
            t.status,
            t.priority
        );
        if let Some(g) = &t.group {
            println!("group: {}", g);
        }
        if !dep_states.is_empty() {
            println!("deps:  {}", dep_states.join(", "));
        }
        println!("created: {}", t.created_at);
        if t.updated_at != t.created_at {
            println!("updated: {}", t.updated_at);
        }
        if let Some(b) = &t.body {
            println!("\n## body\n{}", b);
        }
        if let Some(a) = &t.acceptance {
            println!("\n## acceptance\n{}", a);
        }
    }
    Ok(())
}

fn cmd_ready(group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let by_id: HashMap<i64, &Task> = all.iter().map(|t| (t.id, t)).collect();
    let ready: Vec<&Task> = all
        .iter()
        .filter(|t| t.status == "open")
        .filter(|t| group.as_deref().map_or(true, |g| t.group.as_deref() == Some(g)))
        .filter(|t| {
            t.deps
                .iter()
                .all(|d| by_id.get(d).map_or(true, |x| x.status == "done"))
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&ready)?);
    } else if ready.is_empty() {
        println!("(no ready tasks)");
    } else {
        for t in ready {
            print_task_row(t);
        }
    }
    Ok(())
}

fn cmd_blocked(group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let by_id: HashMap<i64, &Task> = all.iter().map(|t| (t.id, t)).collect();

    #[derive(Serialize)]
    struct BlockedEntry {
        task: Task,
        unmet_deps: Vec<String>,
    }

    let mut entries: Vec<BlockedEntry> = vec![];
    for t in &all {
        if t.status != "open" {
            continue;
        }
        if let Some(g) = &group {
            if t.group.as_deref() != Some(g.as_str()) {
                continue;
            }
        }
        let unmet: Vec<String> = t
            .deps
            .iter()
            .filter_map(|d| match by_id.get(d) {
                Some(x) if x.status == "done" => None,
                Some(x) => Some(format!("{} ({})", fmt_id(*d), x.status)),
                None => Some(format!("{} (missing)", fmt_id(*d))),
            })
            .collect();
        if !unmet.is_empty() {
            entries.push(BlockedEntry {
                task: t.clone(),
                unmet_deps: unmet,
            });
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else if entries.is_empty() {
        println!("(no blocked tasks)");
    } else {
        for e in &entries {
            print_task_row(&e.task);
            println!("       blocked by: {}", e.unmet_deps.join(", "));
        }
    }
    Ok(())
}

fn cmd_status(id: String, state: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = db::set_status(&conn, id, &state)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} -> {}", fmt_id(id), state);
    Ok(())
}

fn cmd_done(id: String) -> Result<()> {
    cmd_status(id, "done".into())
}

fn cmd_rm(id: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = db::delete_task(&conn, id)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} deleted", fmt_id(id));
    Ok(())
}

fn cmd_prompt(name: String) -> Result<()> {
    let body = match name.as_str() {
        "tdd-pursuit" | "tdd" => PROMPT_TDD,
        "create-task" | "create" => PROMPT_CREATE_TASK,
        "commit" => PROMPT_COMMIT,
        "pr" => PROMPT_PR,
        other => {
            return Err(anyhow!(
                "unknown prompt: {}\nknown: tdd-pursuit, create-task, commit, pr",
                other
            ))
        }
    };
    print!("{}", body);
    Ok(())
}

fn mark_in_progress(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;
    Ok(())
}

fn cmd_start(id: String, tmux: bool) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let t = all
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow!("{} not found", fmt_id(id)))?;

    if t.status == "done" {
        return Err(anyhow!(
            "{} is done — run `docket status {} open` first to reopen it",
            fmt_id(t.id),
            fmt_id(t.id)
        ));
    }

    let prompt = assemble_prompt(t);

    if tmux {
        if std::env::var_os("TMUX").is_none() {
            return Err(anyhow!(
                "not inside a tmux session — run `docket start {} | claude` or start tmux first",
                fmt_id(t.id)
            ));
        }
        deliver_via_tmux(t.id, &prompt)?;
        mark_in_progress(&conn, t.id)?;
        return Ok(());
    }

    print!("{}", prompt);
    mark_in_progress(&conn, t.id)?;
    Ok(())
}

fn deliver_via_tmux(id: i64, prompt: &str) -> Result<()> {
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let prompt_path = std::env::temp_dir().join(format!("docket-{}-{}.md", fmt_id(id), ts));
    fs::write(&prompt_path, prompt)
        .with_context(|| format!("failed to write prompt tempfile {}", prompt_path.display()))?;

    let window_name = format!("docket {}", fmt_id(id));
    let new_window = std::process::Command::new("tmux")
        .args(["new-window", "-n", &window_name, "-P", "-F", "#{window_id}"])
        .output()
        .context("failed to invoke tmux new-window")?;
    if !new_window.status.success() {
        return Err(anyhow!(
            "tmux new-window failed: {}",
            String::from_utf8_lossy(&new_window.stderr)
        ));
    }
    let window_id = String::from_utf8_lossy(&new_window.stdout).trim().to_string();
    if window_id.is_empty() {
        return Err(anyhow!("tmux new-window did not return a window id"));
    }

    let cmd = format!("claude < {}", prompt_path.display());
    let send = std::process::Command::new("tmux")
        .args(["send-keys", "-t", &window_id, &cmd, "Enter"])
        .output()
        .context("failed to invoke tmux send-keys")?;
    if !send.status.success() {
        return Err(anyhow!(
            "tmux send-keys failed: {}",
            String::from_utf8_lossy(&send.stderr)
        ));
    }
    Ok(())
}

fn cmd_group_new(
    name: String,
    branch: Option<String>,
    description: Option<String>,
) -> Result<()> {
    let conn = open_db()?;
    if load_group_by_name(&conn, &name)?.is_some() {
        return Err(anyhow!("group `{}` already exists", name));
    }
    conn.execute(
        "INSERT INTO groups (name, branch_name, description, state, created_at)
         VALUES (?1, ?2, ?3, 'open', ?4)",
        params![name, branch, description, now()],
    )?;
    println!("group `{}` created", name);
    Ok(())
}

fn cmd_group_ls(json: bool) -> Result<()> {
    let conn = open_db()?;
    let groups = load_all_groups(&conn)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&groups)?);
        return Ok(());
    }

    if groups.is_empty() {
        println!("(no groups)");
        return Ok(());
    }

    for g in &groups {
        let counts: (i64, i64, i64) = conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'done' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'open' THEN 1 ELSE 0 END), 0)
             FROM tasks WHERE group_id = ?1",
            params![g.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let branch_str = g
            .branch_name
            .as_deref()
            .map(|b| format!(" branch={}", b))
            .unwrap_or_default();
        println!(
            "{:<20} [{}]  {}/{} done{}",
            g.name, g.state, counts.1, counts.0, branch_str
        );
    }
    Ok(())
}

fn cmd_group_show(name: String, json: bool) -> Result<()> {
    let conn = open_db()?;
    let g = load_group_by_name(&conn, &name)?
        .ok_or_else(|| anyhow!("group `{}` not found", name))?;

    let all = load_all_tasks(&conn)?;
    let group_tasks: Vec<&Task> = all
        .iter()
        .filter(|t| t.group.as_deref() == Some(g.name.as_str()))
        .collect();

    if json {
        #[derive(Serialize)]
        struct GroupShow<'a> {
            group: &'a Group,
            tasks: Vec<&'a Task>,
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&GroupShow {
                group: &g,
                tasks: group_tasks
            })?
        );
    } else {
        println!("group: {}  [{}]", g.name, g.state);
        if let Some(b) = &g.branch_name {
            println!("branch: {}", b);
        }
        if let Some(d) = &g.description {
            println!("description: {}", d);
        }
        println!("created: {}", g.created_at);
        println!();
        if group_tasks.is_empty() {
            println!("(no tasks in this group)");
        } else {
            for t in &group_tasks {
                print_task_row(t);
            }
        }
    }
    Ok(())
}

fn cmd_group_close(name: String) -> Result<()> {
    let conn = open_db()?;
    let n = conn.execute(
        "UPDATE groups SET state = 'closed' WHERE name = ?1",
        params![name],
    )?;
    if n == 0 {
        return Err(anyhow!("group `{}` not found", name));
    }
    println!("group `{}` closed", name);
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => cmd_init()?,
        Command::Add {
            title,
            body,
            acceptance,
            deps,
            priority,
            group,
        } => cmd_add(title, body, acceptance, deps, priority, group)?,
        Command::Ls { status, group, json } => cmd_ls(status, group, json)?,
        Command::Show { id, json } => cmd_show(id, json)?,
        Command::Ready { group, json } => cmd_ready(group, json)?,
        Command::Blocked { group, json } => cmd_blocked(group, json)?,
        Command::Status { id, state } => cmd_status(id, state)?,
        Command::Done { id } => cmd_done(id)?,
        Command::Rm { id } => cmd_rm(id)?,
        Command::Prompt { name } => cmd_prompt(name)?,
        Command::Start { id, tmux } => cmd_start(id, tmux)?,
        Command::Group { action } => match action {
            GroupCommand::New {
                name,
                branch,
                description,
            } => cmd_group_new(name, branch, description)?,
            GroupCommand::Ls { json } => cmd_group_ls(json)?,
            GroupCommand::Show { name, json } => cmd_group_show(name, json)?,
            GroupCommand::Close { name } => cmd_group_close(name)?,
        },
    }
    Ok(())
}

