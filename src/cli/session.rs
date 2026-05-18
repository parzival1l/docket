use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::agent_session_info::{find_session_uuid_for_task, placeholder_task_id};
use crate::cli::start::spawn_tmux_session;
use crate::db::{
    link_session, open_db, placeholder_session_links, relink_session, session_task, unlink_session,
};
use crate::model::{fmt_id, parse_id};

pub fn link(task_id: String, session_id: String) -> Result<()> {
    let task_id = parse_id(&task_id)?;
    let conn = open_db()?;
    link_session(&conn, task_id, &session_id)?;
    println!("linked {} -> {}", session_id, fmt_id(task_id));
    Ok(())
}

pub fn unlink(session_id: String) -> Result<()> {
    let conn = open_db()?;
    let n = unlink_session(&conn, &session_id)?;
    if n == 0 {
        println!("session {} was not linked", session_id);
    } else {
        println!("unlinked {}", session_id);
    }
    Ok(())
}

/// Spawn a fresh tmux session that runs `claude --resume <session_id>` and
/// attach to it (switch-client inside tmux, or open a new terminal window
/// via $DOCKET_TERMINAL_CMD when outside). `force_spawn` mirrors the same
/// flag on `start --tmux` — the TUI uses it because its own window is busy.
pub fn open(session_id: String, force_spawn: bool) -> Result<()> {
    if session_id.trim().is_empty() {
        return Err(anyhow!("session id is empty"));
    }
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let short: String = session_id.chars().take(8).collect();
    let tmux_name = format!("docket-open-{}-{}", short, ts);
    let cmd = format!("claude --resume {}", shell_escape(&session_id));
    spawn_tmux_session(&tmux_name, &cmd, force_spawn)
}

fn shell_escape(s: &str) -> String {
    // Session IDs we mint are uuids or `docket-...` names — both shell-safe.
    // Defensive single-quote wrapping handles anything a human might link.
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Find rows whose `session_id` is still the old `docket-T-N-<ts>` tmux
/// placeholder (written before `docket start --tmux` switched to passing
/// `--session-id <uuid>` to claude). For each, scan the cwd's claude
/// transcripts looking for the prompt header `# Task T-{N}:` and, on a hit,
/// rewrite the row's session_id to the transcript's real uuid.
///
/// `limit` caps how many recent transcripts to inspect per task — smaller
/// is faster but risks missing a hit if the right transcript was bumped out
/// by unrelated newer sessions.
pub fn reconcile(limit: usize, dry_run: bool) -> Result<()> {
    let conn = open_db()?;
    let stale = placeholder_session_links(&conn)?;
    if stale.is_empty() {
        println!("no placeholder session links to reconcile");
        return Ok(());
    }
    let mut hits = 0usize;
    let mut misses: Vec<(String, i64)> = Vec::new();
    for (old_id, task_id) in stale {
        // Belt-and-braces: the placeholder name embeds the task id; if the row's
        // task_id disagrees, the row is corrupted and we'd write the wrong uuid
        // for the wrong task. Skip instead.
        if let Some(embedded) = placeholder_task_id(&old_id) {
            if embedded != task_id {
                println!(
                    "skipping {}: embeds task {} but row says task {}",
                    old_id,
                    fmt_id(embedded),
                    fmt_id(task_id)
                );
                continue;
            }
        }
        match find_session_uuid_for_task(task_id, limit) {
            Some(uuid) => {
                if dry_run {
                    println!(
                        "would rewrite {} → {} (task {})",
                        old_id,
                        uuid,
                        fmt_id(task_id)
                    );
                } else {
                    relink_session(&conn, &old_id, &uuid)?;
                    println!("rewrote {} → {} (task {})", old_id, uuid, fmt_id(task_id));
                }
                hits += 1;
            }
            None => misses.push((old_id, task_id)),
        }
    }
    for (id, task_id) in &misses {
        println!("no transcript match for {} (task {})", id, fmt_id(*task_id));
    }
    println!(
        "reconcile{}: {} rewritten, {} unmatched",
        if dry_run { " (dry-run)" } else { "" },
        hits,
        misses.len()
    );
    Ok(())
}

pub fn show(session_id: String, json: bool) -> Result<()> {
    let conn = open_db()?;
    let task = session_task(&conn, &session_id)?;
    if json {
        let body = match task {
            Some(id) => serde_json::json!({
                "session_id": session_id,
                "task": fmt_id(id),
            }),
            None => serde_json::json!({
                "session_id": session_id,
                "task": serde_json::Value::Null,
            }),
        };
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        match task {
            Some(id) => println!("{} -> {}", session_id, fmt_id(id)),
            None => println!("{} -> (not linked)", session_id),
        }
    }
    Ok(())
}
