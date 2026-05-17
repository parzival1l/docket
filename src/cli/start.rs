use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::fs;

use crate::db::{link_session, load_all_tasks, open_db};
use crate::model::{fmt_id, now, parse_id};
use crate::prompts::assemble_prompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmuxDelivery {
    Off,
    Auto,
    ForceSpawn,
}

pub fn run(id: String, delivery: TmuxDelivery) -> Result<()> {
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

    match delivery {
        TmuxDelivery::Off => {
            print!("{}", prompt);
        }
        TmuxDelivery::Auto | TmuxDelivery::ForceSpawn => {
            let force_spawn = matches!(delivery, TmuxDelivery::ForceSpawn);
            let session_name = deliver_via_tmux_session(t.id, &prompt, force_spawn)?;
            link_session(&conn, t.id, &session_name)?;
        }
    }

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

fn deliver_via_tmux_session(id: i64, prompt: &str, force_spawn: bool) -> Result<String> {
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let prompt_path = std::env::temp_dir().join(format!("docket-{}-{}.md", fmt_id(id), ts));
    fs::write(&prompt_path, prompt)
        .with_context(|| format!("failed to write prompt tempfile {}", prompt_path.display()))?;

    let session_name = format!("docket-{}-{}", fmt_id(id), ts);
    let cmd = format!("claude < {}", prompt_path.display());
    spawn_tmux_session(&session_name, &cmd, force_spawn)?;
    Ok(session_name)
}

/// Create a detached tmux session named `session_name`, send `command` into
/// it, then either switch-client (when inside tmux and force_spawn=false) or
/// spawn an external terminal that attaches to it. The session name is
/// returned so callers can record it.
pub fn spawn_tmux_session(session_name: &str, command: &str, force_spawn: bool) -> Result<()> {
    let new_session = std::process::Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            session_name,
            "-P",
            "-F",
            "#{session_id}",
        ])
        .output()
        .context("failed to invoke tmux new-session")?;
    if !new_session.status.success() {
        return Err(anyhow!(
            "tmux new-session failed: {}",
            String::from_utf8_lossy(&new_session.stderr)
        ));
    }
    let session_id = String::from_utf8_lossy(&new_session.stdout)
        .trim()
        .to_string();
    if session_id.is_empty() {
        return Err(anyhow!("tmux new-session did not return a session id"));
    }

    let send = std::process::Command::new("tmux")
        .args(["send-keys", "-t", &session_id, command, "Enter"])
        .output()
        .context("failed to invoke tmux send-keys")?;
    if !send.status.success() {
        return Err(anyhow!(
            "tmux send-keys failed: {}",
            String::from_utf8_lossy(&send.stderr)
        ));
    }

    let inside_tmux = std::env::var_os("TMUX").is_some();
    if inside_tmux && !force_spawn {
        let switch = std::process::Command::new("tmux")
            .args(["switch-client", "-t", &session_id])
            .output()
            .context("failed to invoke tmux switch-client")?;
        if !switch.status.success() {
            return Err(anyhow!(
                "tmux switch-client failed: {}",
                String::from_utf8_lossy(&switch.stderr)
            ));
        }
    } else {
        spawn_terminal_with_attach(session_name)?;
    }

    Ok(())
}

fn spawn_terminal_with_attach(session_name: &str) -> Result<()> {
    let attach_cmd = format!("tmux attach -t {}", session_name);
    let spawn_template = resolve_terminal_spawner()?;
    let expanded = spawn_template.replace("{cmd}", &attach_cmd);

    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&expanded)
        .output()
        .with_context(|| format!("failed to spawn terminal via: {}", expanded))?;
    if !out.status.success() {
        return Err(anyhow!(
            "terminal spawn failed (cmd: {}): {}",
            expanded,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn resolve_terminal_spawner() -> Result<String> {
    if let Ok(custom) = std::env::var("DOCKET_TERMINAL_CMD") {
        if !custom.trim().is_empty() {
            if !custom.contains("{cmd}") {
                return Err(anyhow!(
                    "$DOCKET_TERMINAL_CMD must contain '{{cmd}}' placeholder; got: {}",
                    custom
                ));
            }
            return Ok(custom);
        }
    }

    if let Ok(prog) = std::env::var("TERM_PROGRAM") {
        if let Some(tpl) = builtin_terminal_template(&prog) {
            return Ok(tpl.to_string());
        }
    }

    Err(anyhow!(
        "could not determine how to spawn a terminal window. \
         Set $DOCKET_TERMINAL_CMD to a shell command containing '{{cmd}}', e.g.:\n  \
         export DOCKET_TERMINAL_CMD='open -na Ghostty.app --args -e {{cmd}}'\n\
         (the {{cmd}} placeholder will be replaced with `tmux attach -t <session>`)"
    ))
}

fn builtin_terminal_template(term_program: &str) -> Option<&'static str> {
    match term_program {
        "ghostty" | "Ghostty" => Some("open -na Ghostty.app --args -e {cmd}"),
        "iTerm.app" => Some("open -na iTerm.app --args -e {cmd}"),
        "Apple_Terminal" => {
            Some("osascript -e 'tell application \"Terminal\" to do script \"{cmd}\"'")
        }
        "WezTerm" => Some("wezterm start -- sh -c '{cmd}; exec $SHELL'"),
        _ => None,
    }
}
