//! Best-effort enrichment of an agent session id by reading Claude Code's
//! local transcript files at `~/.claude/projects/*/<session_id>.jsonl`.
//!
//! Used by the TUI's session picker to differentiate multiple sessions on the
//! same task — title, user-turn count, and last activity. If the transcript
//! isn't found (e.g. the id is an older `docket-T-N-<ts>` tmux name, or the
//! file was deleted), the fields stay `None` and the caller renders a
//! `linked X ago` fallback from the db's `linked_at`.

use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    /// Claude's auto-generated title for the session, if present.
    pub title: Option<String>,
    /// Count of `type:"user"` events in the transcript.
    pub turns: Option<usize>,
    /// File mtime of the transcript.
    pub last_active: Option<DateTime<Utc>>,
    /// Linked_at from the docket db (always present for linked sessions).
    pub linked_at: Option<DateTime<Utc>>,
}

impl SessionInfo {
    pub fn unknown(id: String, linked_at: Option<DateTime<Utc>>) -> Self {
        SessionInfo {
            id,
            title: None,
            turns: None,
            last_active: None,
            linked_at,
        }
    }
}

/// Look up a session in Claude's project transcripts. Cheap: stat-only scan
/// across `~/.claude/projects/*/`. Returns enriched info if a matching
/// transcript exists, otherwise an `unknown` shell that still carries
/// `linked_at`.
pub fn inspect(session_id: &str, linked_at: Option<DateTime<Utc>>) -> SessionInfo {
    let path = match locate_transcript(session_id) {
        Some(p) => p,
        None => return SessionInfo::unknown(session_id.to_string(), linked_at),
    };
    let last_active = fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(systemtime_to_utc);
    let (title, turns) = scan_transcript(&path);
    SessionInfo {
        id: session_id.to_string(),
        title,
        turns,
        last_active,
        linked_at,
    }
}

fn locate_transcript(session_id: &str) -> Option<PathBuf> {
    let projects = projects_root()?;
    let target = format!("{}.jsonl", session_id);
    // Try the current repo's project dir first so the common case is one stat.
    if let Some(cwd_dir) = cwd_project_dir(&projects) {
        let p = cwd_dir.join(&target);
        if p.is_file() {
            return Some(p);
        }
    }
    // Fall back to scanning every project dir. Cheap for typical inventories.
    let entries = fs::read_dir(&projects).ok()?;
    for entry in entries.flatten() {
        let p = entry.path().join(&target);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn projects_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home).join(".claude").join("projects");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

fn cwd_project_dir(projects: &PathBuf) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let encoded: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    let p = projects.join(encoded);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// Single-pass scan: pick the last `ai-title.aiTitle` and count user turns.
fn scan_transcript(path: &PathBuf) -> (Option<String>, Option<usize>) {
    let content = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return (None, None),
    };
    let mut title: Option<String> = None;
    let mut turns: usize = 0;
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("user") => turns += 1,
            Some("ai-title") => {
                if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()) {
                    title = Some(t.to_string());
                }
            }
            _ => {}
        }
    }
    (title, Some(turns))
}

fn systemtime_to_utc(t: SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(t)
}

/// Compact "relative ago" formatter — "12s", "3m", "2h", "5d".
pub fn relative_ago(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - when).num_seconds();
    if secs < 0 {
        return "just now".into();
    }
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 60 * 60 {
        format!("{}m ago", secs / 60)
    } else if secs < 60 * 60 * 24 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_ago_buckets() {
        let now = Utc::now();
        assert_eq!(relative_ago(now - chrono::Duration::seconds(5), now), "5s ago");
        assert_eq!(relative_ago(now - chrono::Duration::seconds(90), now), "1m ago");
        assert_eq!(relative_ago(now - chrono::Duration::seconds(3700), now), "1h ago");
        assert_eq!(relative_ago(now - chrono::Duration::seconds(90_000), now), "1d ago");
    }

    #[test]
    fn unknown_carries_linked_at() {
        let lat = Utc::now();
        let info = SessionInfo::unknown("nope".into(), Some(lat));
        assert_eq!(info.id, "nope");
        assert!(info.title.is_none());
        assert!(info.turns.is_none());
        assert!(info.last_active.is_none());
        assert_eq!(info.linked_at, Some(lat));
    }
}
