use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::Parser;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;

mod cli;
mod db;
mod model;
mod prompts;

use cli::print_task_row;
use db::{get_or_create_group, load_all_groups, load_all_tasks, load_group_by_name, open_db};
use model::{fmt_id, now, parse_deps, parse_id, Group, Task};
use prompts::{assemble_prompt, PROMPT_COMMIT, PROMPT_CREATE_TASK, PROMPT_PR, PROMPT_TDD};

// === commands ===

pub(crate) fn cmd_blocked(group: Option<String>, json: bool) -> Result<()> {
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

pub(crate) fn cmd_status(id: String, state: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = db::set_status(&conn, id, &state)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} -> {}", fmt_id(id), state);
    Ok(())
}

pub(crate) fn cmd_done(id: String) -> Result<()> {
    cmd_status(id, "done".into())
}

pub(crate) fn cmd_rm(id: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = db::delete_task(&conn, id)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} deleted", fmt_id(id));
    Ok(())
}

pub(crate) fn cmd_prompt(name: String) -> Result<()> {
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

pub(crate) fn cmd_start(id: String, tmux: bool) -> Result<()> {
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

pub(crate) fn cmd_group_new(
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

pub(crate) fn cmd_group_ls(json: bool) -> Result<()> {
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

pub(crate) fn cmd_group_show(name: String, json: bool) -> Result<()> {
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

pub(crate) fn cmd_group_close(name: String) -> Result<()> {
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
    let args = cli::Cli::parse();
    cli::dispatch(args)
}

