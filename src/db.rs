use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::model::{deps_from_db, now, Group, Task};

pub const SCHEMA: &str = r#"
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

pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
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

pub fn docket_dir() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let root = find_repo_root(&cwd)
        .ok_or_else(|| anyhow!("not in a git repo (run from inside one, or `git init` first)"))?;
    Ok(root.join(".docket"))
}

pub fn open_db() -> Result<Connection> {
    let path = docket_dir()?.join("db.sqlite");
    if !path.exists() {
        return Err(anyhow!(
            ".docket/ not initialized in this repo — run `docket init` first"
        ));
    }
    Ok(Connection::open(&path)?)
}

pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

pub fn load_all_tasks(conn: &Connection) -> Result<Vec<Task>> {
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

pub fn load_group_by_name(conn: &Connection, name: &str) -> Result<Option<Group>> {
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
        .optional()?;
    Ok(g)
}

pub fn load_all_groups(conn: &Connection) -> Result<Vec<Group>> {
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
    Ok(groups)
}

pub fn get_or_create_group(conn: &Connection, name: &str) -> Result<i64> {
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

pub struct NewTask<'a> {
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub acceptance: Option<&'a str>,
    pub deps_json: Option<&'a str>,
    pub priority: i32,
    pub group_id: Option<i64>,
}

pub fn insert_task(conn: &Connection, t: NewTask) -> Result<i64> {
    let n = now();
    conn.execute(
        "INSERT INTO tasks (title, body, acceptance, deps, status, priority, group_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'open', ?5, ?6, ?7, ?7)",
        params![t.title, t.body, t.acceptance, t.deps_json, t.priority, t.group_id, n],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Returns the number of rows updated (0 if id not found).
pub fn set_status(conn: &Connection, id: i64, state: &str) -> Result<usize> {
    let n = conn.execute(
        "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![state, now(), id],
    )?;
    Ok(n)
}

/// Returns the number of rows deleted (0 if id not found).
pub fn delete_task(conn: &Connection, id: i64) -> Result<usize> {
    let n = conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
    Ok(n)
}

#[allow(dead_code)] // used by TUI (lands in PR-3)
pub struct TaskUpdate<'a> {
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub acceptance: Option<&'a str>,
    pub deps_json: Option<&'a str>,
    pub priority: i32,
    pub group_id: Option<i64>,
}

/// Updates all editable fields on a task. `status` is intentionally not
/// touched here — use `set_status` for that.
#[allow(dead_code)] // used by TUI (lands in PR-3)
pub fn update_task(conn: &Connection, id: i64, t: TaskUpdate) -> Result<usize> {
    let n = conn.execute(
        "UPDATE tasks SET title = ?1, body = ?2, acceptance = ?3, deps = ?4,
                          priority = ?5, group_id = ?6, updated_at = ?7
         WHERE id = ?8",
        params![t.title, t.body, t.acceptance, t.deps_json, t.priority, t.group_id, now(), id],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init_schema(&c).unwrap();
        c
    }

    #[test]
    fn init_schema_is_idempotent() {
        let c = mem_conn();
        init_schema(&c).unwrap();
        init_schema(&c).unwrap();
    }

    #[test]
    fn load_all_tasks_empty_returns_empty_vec() {
        let c = mem_conn();
        assert!(load_all_tasks(&c).unwrap().is_empty());
    }

    #[test]
    fn get_or_create_group_creates_then_returns_existing() {
        let c = mem_conn();
        let id1 = get_or_create_group(&c, "v0.1").unwrap();
        let id2 = get_or_create_group(&c, "v0.1").unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn load_all_groups_returns_sorted_by_name() {
        let c = mem_conn();
        get_or_create_group(&c, "zeta").unwrap();
        get_or_create_group(&c, "alpha").unwrap();
        get_or_create_group(&c, "mu").unwrap();
        let names: Vec<String> = load_all_groups(&c)
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn load_group_by_name_returns_none_when_missing() {
        let c = mem_conn();
        assert!(load_group_by_name(&c, "ghost").unwrap().is_none());
    }

    fn seed_one(c: &Connection) -> i64 {
        insert_task(
            c,
            NewTask {
                title: "x",
                body: None,
                acceptance: None,
                deps_json: None,
                priority: 2,
                group_id: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn insert_task_returns_new_id_and_round_trips() {
        let c = mem_conn();
        let id = insert_task(
            &c,
            NewTask {
                title: "hello",
                body: Some("b"),
                acceptance: Some("a"),
                deps_json: Some("[1,2]"),
                priority: 1,
                group_id: None,
            },
        )
        .unwrap();
        assert_eq!(id, 1);
        let tasks = load_all_tasks(&c).unwrap();
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.title, "hello");
        assert_eq!(t.body.as_deref(), Some("b"));
        assert_eq!(t.acceptance.as_deref(), Some("a"));
        assert_eq!(t.deps, vec![1, 2]);
        assert_eq!(t.priority, 1);
        assert_eq!(t.status, "open");
    }

    #[test]
    fn set_status_updates_existing_row() {
        let c = mem_conn();
        let id = seed_one(&c);
        let n = set_status(&c, id, "in_progress").unwrap();
        assert_eq!(n, 1);
        let t = &load_all_tasks(&c).unwrap()[0];
        assert_eq!(t.status, "in_progress");
    }

    #[test]
    fn set_status_returns_zero_for_missing_id() {
        let c = mem_conn();
        let n = set_status(&c, 9999, "done").unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn delete_task_removes_row() {
        let c = mem_conn();
        let id = seed_one(&c);
        let n = delete_task(&c, id).unwrap();
        assert_eq!(n, 1);
        assert!(load_all_tasks(&c).unwrap().is_empty());
    }

    #[test]
    fn delete_task_returns_zero_for_missing_id() {
        let c = mem_conn();
        let n = delete_task(&c, 9999).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn update_task_changes_all_fields() {
        let c = mem_conn();
        let id = seed_one(&c);
        let n = update_task(
            &c,
            id,
            TaskUpdate {
                title: "y",
                body: Some("nb"),
                acceptance: Some("na"),
                deps_json: Some("[3]"),
                priority: 0,
                group_id: None,
            },
        )
        .unwrap();
        assert_eq!(n, 1);
        let t = &load_all_tasks(&c).unwrap()[0];
        assert_eq!(t.title, "y");
        assert_eq!(t.body.as_deref(), Some("nb"));
        assert_eq!(t.acceptance.as_deref(), Some("na"));
        assert_eq!(t.deps, vec![3]);
        assert_eq!(t.priority, 0);
    }

    #[test]
    fn update_task_does_not_touch_status() {
        let c = mem_conn();
        let id = seed_one(&c);
        set_status(&c, id, "in_progress").unwrap();
        update_task(
            &c,
            id,
            TaskUpdate {
                title: "y",
                body: None,
                acceptance: None,
                deps_json: None,
                priority: 2,
                group_id: None,
            },
        )
        .unwrap();
        let t = &load_all_tasks(&c).unwrap()[0];
        assert_eq!(t.status, "in_progress");
    }

    #[test]
    fn update_task_returns_zero_for_missing_id() {
        let c = mem_conn();
        let n = update_task(
            &c,
            9999,
            TaskUpdate {
                title: "y",
                body: None,
                acceptance: None,
                deps_json: None,
                priority: 2,
                group_id: None,
            },
        )
        .unwrap();
        assert_eq!(n, 0);
    }
}
