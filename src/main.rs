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

