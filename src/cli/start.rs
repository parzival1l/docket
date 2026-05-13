use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::fs;

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, now, parse_id};
use crate::prompts::assemble_prompt;

pub fn run(id: String, tmux: bool) -> Result<()> {
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

fn mark_in_progress(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status = 'in_progress', updated_at = ?1 WHERE id = ?2",
        params![now(), id],
    )?;
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
