use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS groups (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL UNIQUE,
    branch_name  TEXT,
    description  TEXT,
    state        TEXT NOT NULL DEFAULT 'open',
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    title       TEXT NOT NULL,
    body        TEXT,
    acceptance  TEXT,
    deps        TEXT,
    status      TEXT NOT NULL DEFAULT 'open',
    priority    INTEGER NOT NULL DEFAULT 2,
    group_id    INTEGER REFERENCES groups(id),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_group  ON tasks(group_id);
"#;

const PROMPT_TDD: &str = include_str!("../templates/tdd-pursuit.md");
const PROMPT_CREATE_TASK: &str = include_str!("../templates/create-task.md");
const PROMPT_COMMIT: &str = include_str!("../templates/commit.md");
const PROMPT_PR: &str = include_str!("../templates/pr.md");

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
    Start { id: String },
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

#[derive(Serialize, Clone)]
struct Task {
    id: i64,
    title: String,
    body: Option<String>,
    acceptance: Option<String>,
    deps: Vec<i64>,
    status: String,
    priority: i32,
    group: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Clone)]
struct Group {
    id: i64,
    name: String,
    branch_name: Option<String>,
    description: Option<String>,
    state: String,
    created_at: String,
}

// === helpers ===

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn fmt_id(id: i64) -> String {
    format!("T-{}", id)
}

fn parse_id(s: &str) -> Result<i64> {
    let stripped = s
        .trim()
        .trim_start_matches(['T', 't'])
        .trim_start_matches('-');
    stripped
        .parse::<i64>()
        .with_context(|| format!("invalid task id: {}", s))
}

fn parse_deps(input: Option<String>) -> Result<Option<String>> {
    match input {
        None => Ok(None),
        Some(s) => {
            let ids: Result<Vec<i64>> = s
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|x| !x.is_empty())
                .map(parse_id)
                .collect();
            let ids = ids?;
            if ids.is_empty() {
                Ok(None)
            } else {
                Ok(Some(serde_json::to_string(&ids)?))
            }
        }
    }
}

fn deps_from_db(s: Option<String>) -> Vec<i64> {
    match s {
        None => vec![],
        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut p = start.to_path_buf();
    loop {
        if p.join(".git").exists() {
            return Some(p);
        }
        if !p.pop() {
            return None;
        }
    }
}

fn docket_dir() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd)
        .ok_or_else(|| anyhow!("not in a git repo (run from inside one, or `git init` first)"))?;
    Ok(root.join(".docket"))
}

fn open_db() -> Result<Connection> {
    let path = docket_dir()?.join("db.sqlite");
    if !path.exists() {
        return Err(anyhow!(
            ".docket/ not initialized in this repo — run `docket init` first"
        ));
    }
    Ok(Connection::open(&path)?)
}

fn load_all_tasks(conn: &Connection) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, t.body, t.acceptance, t.deps, t.status, t.priority,
                g.name, t.created_at, t.updated_at
         FROM tasks t LEFT JOIN groups g ON t.group_id = g.id
         ORDER BY t.priority, t.id",
    )?;
    let tasks = stmt
        .query_map([], |r| {
            let deps_raw: Option<String> = r.get(4)?;
            Ok(Task {
                id: r.get(0)?,
                title: r.get(1)?,
                body: r.get(2)?,
                acceptance: r.get(3)?,
                deps: deps_from_db(deps_raw),
                status: r.get(5)?,
                priority: r.get(6)?,
                group: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tasks)
}

fn get_or_create_group(conn: &Connection, name: &str) -> Result<i64> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM groups WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => Ok(id),
        None => {
            conn.execute(
                "INSERT INTO groups (name, state, created_at) VALUES (?1, 'open', ?2)",
                params![name, now()],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }
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
    conn.execute_batch(SCHEMA)?;
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
    let n = now();
    conn.execute(
        "INSERT INTO tasks (title, body, acceptance, deps, status, priority, group_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, ?7, ?7)",
        params![title, body, acceptance, deps_json, priority, group_id, n],
    )?;
    let id = conn.last_insert_rowid();
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
    let n = conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![state, now(), id],
    )?;
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
    let n = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
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

fn cmd_start(id: String) -> Result<()> {
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

    let body = t.body.as_deref().unwrap_or("(no body)");
    let acceptance = t.acceptance.as_deref().unwrap_or("(no acceptance criteria)");

    print!(
        "# Task {}: {}\n\n## Body\n{}\n\n## Acceptance\n{}\n\n{}",
        fmt_id(t.id),
        t.title,
        body,
        acceptance,
        PROMPT_TDD
    );

    conn.execute(
        "UPDATE tasks SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
        params![now(), t.id],
    )?;
    Ok(())
}

fn cmd_group_new(
    name: String,
    branch: Option<String>,
    description: Option<String>,
) -> Result<()> {
    let conn = open_db()?;
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM groups WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
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
    let mut stmt = conn.prepare(
        "SELECT id, name, branch_name, description, state, created_at FROM groups ORDER BY name",
    )?;
    let groups: Vec<Group> = stmt
        .query_map([], |r| {
            Ok(Group {
                id: r.get(0)?,
                name: r.get(1)?,
                branch_name: r.get(2)?,
                description: r.get(3)?,
                state: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

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
    let g = conn
        .query_row(
            "SELECT id, name, branch_name, description, state, created_at FROM groups WHERE name = ?1",
            params![name],
            |r| {
                Ok(Group {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    branch_name: r.get(2)?,
                    description: r.get(3)?,
                    state: r.get(4)?,
                    created_at: r.get(5)?,
                })
            },
        )
        .optional()?
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
        Command::Start { id } => cmd_start(id)?,
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
