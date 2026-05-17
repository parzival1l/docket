# Docket TUI — PR-1: Module Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `src/main.rs` (845 lines, one file) into focused modules (`model`, `prompts`, `db`, `cli/*`) with **no observable behaviour change**, so subsequent PRs can land a TUI without re-touching CLI code.

**Architecture:** Behaviour-preserving refactor. The existing integration test suite (`tests/start.rs`, 11 tests) is the primary safety net. Each extraction step is small, compiles, and keeps every existing test green. Each new module also lands with unit tests that pin its interface — these tests outlive the refactor and become the contract the TUI consumes.

**Tech Stack:** Rust 2021, `clap` (derive), `rusqlite` (bundled), `anyhow`, `chrono`, `serde`/`serde_json`. No new dependencies in this PR.

---

## File Structure (target after this PR)

```
src/
  main.rs            # ~25 lines — clap parse → cli::dispatch
  model.rs           # Task, Group, fmt_id, parse_id, parse_deps, deps_from_db, now
  prompts.rs         # PROMPT_* constants, assemble_prompt
  db.rs              # SCHEMA, find_repo_root, docket_dir, open_db,
                     # load_all_tasks, get_or_create_group,
                     # insert_task, update_task, set_status, delete_task
  cli/
    mod.rs           # Cli, Command, GroupCommand (clap derive) + dispatch()
    init.rs          # cmd_init
    add.rs           # cmd_add
    ls.rs            # cmd_ls
    show.rs          # cmd_show
    ready.rs         # cmd_ready
    blocked.rs       # cmd_blocked
    status.rs        # cmd_status, cmd_done
    rm.rs            # cmd_rm
    prompt.rs        # cmd_prompt
    start.rs         # cmd_start, deliver_via_tmux
    group.rs         # cmd_group_new, cmd_group_ls, cmd_group_show, cmd_group_close
tests/
  start.rs           # unchanged
```

**One file per CLI verb** matches the spec and keeps each handler small enough to hold in context.

**Notes:**

- No library crate yet — modules live under the binary target via `mod foo;` in `main.rs`. Adding `[lib]` is deferred until something outside this crate wants to import (none planned).
- Unit tests live inline (`#[cfg(test)] mod tests { ... }`) per module — simpler than a parallel `tests/` tree for a binary crate.
- Some new `db.rs` functions (`insert_task`, `update_task`, `delete_task`) don't have CLI callers yet but exist for the TUI. They land in this PR with tests so we never need to round-trip through `main.rs` to add them later.

---

## Task 1: Pin model behaviour with unit tests (before any moves)

**Files:**
- Modify: `src/main.rs:175-217` (add `#[cfg(test)] mod model_tests` at end of file)

**Why first:** the existing integration tests cover end-to-end behaviour but don't pin the parsers directly. Lock them in before moving anything.

- [ ] **Step 1.1: Append model tests to `src/main.rs`**

Open `src/main.rs` and append at end of file:

```rust
#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn fmt_id_formats_with_t_prefix() {
        assert_eq!(fmt_id(1), "T-1");
        assert_eq!(fmt_id(42), "T-42");
    }

    #[test]
    fn parse_id_accepts_t_prefix_dash() {
        assert_eq!(parse_id("T-7").unwrap(), 7);
        assert_eq!(parse_id("t-7").unwrap(), 7);
    }

    #[test]
    fn parse_id_accepts_bare_int() {
        assert_eq!(parse_id("9").unwrap(), 9);
        assert_eq!(parse_id("  9  ").unwrap(), 9);
    }

    #[test]
    fn parse_id_rejects_garbage() {
        assert!(parse_id("foo").is_err());
        assert!(parse_id("T-").is_err());
        assert!(parse_id("").is_err());
    }

    #[test]
    fn parse_deps_none_returns_none() {
        assert_eq!(parse_deps(None).unwrap(), None);
    }

    #[test]
    fn parse_deps_empty_string_returns_none() {
        assert_eq!(parse_deps(Some("".into())).unwrap(), None);
        assert_eq!(parse_deps(Some("   ".into())).unwrap(), None);
    }

    #[test]
    fn parse_deps_comma_separated() {
        let got = parse_deps(Some("T-3,T-5,9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn parse_deps_whitespace_separated() {
        let got = parse_deps(Some("T-3 5 9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn deps_from_db_none_is_empty() {
        assert!(deps_from_db(None).is_empty());
    }

    #[test]
    fn deps_from_db_parses_json_array() {
        assert_eq!(deps_from_db(Some("[1,2,3]".into())), vec![1, 2, 3]);
    }

    #[test]
    fn deps_from_db_invalid_json_is_empty() {
        assert_eq!(deps_from_db(Some("not-json".into())), Vec::<i64>::new());
    }
}
```

- [ ] **Step 1.2: Run unit tests to verify they pass**

Run: `cargo test --bin docket model_tests`
Expected: 11 passed.

- [ ] **Step 1.3: Run the full suite to verify nothing else regressed**

Run: `cargo test`
Expected: all tests pass (11 integration + 11 new unit = 22 total).

- [ ] **Step 1.4: Commit**

```bash
git add src/main.rs
git commit -m "refactor: pin parse_id / parse_deps / deps_from_db with unit tests"
```

---

## Task 2: Extract `model.rs`

**Files:**
- Create: `src/model.rs`
- Modify: `src/main.rs` — declare module, remove moved items, update use-sites

- [ ] **Step 2.1: Create `src/model.rs` with the moved types and helpers**

Write `src/model.rs`:

```rust
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub deps: Vec<i64>,
    pub status: String,
    pub priority: i32,
    pub group: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub branch_name: Option<String>,
    pub description: Option<String>,
    pub state: String,
    pub created_at: String,
}

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn fmt_id(id: i64) -> String {
    format!("T-{}", id)
}

pub fn parse_id(s: &str) -> Result<i64> {
    let stripped = s
        .trim()
        .trim_start_matches(['T', 't'])
        .trim_start_matches('-');
    stripped
        .parse::<i64>()
        .with_context(|| format!("invalid task id: {}", s))
}

pub fn parse_deps(input: Option<String>) -> Result<Option<String>> {
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

pub fn deps_from_db(s: Option<String>) -> Vec<i64> {
    match s {
        None => vec![],
        Some(s) => serde_json::from_str(&s).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_id_formats_with_t_prefix() {
        assert_eq!(fmt_id(1), "T-1");
        assert_eq!(fmt_id(42), "T-42");
    }

    #[test]
    fn parse_id_accepts_t_prefix_dash() {
        assert_eq!(parse_id("T-7").unwrap(), 7);
        assert_eq!(parse_id("t-7").unwrap(), 7);
    }

    #[test]
    fn parse_id_accepts_bare_int() {
        assert_eq!(parse_id("9").unwrap(), 9);
        assert_eq!(parse_id("  9  ").unwrap(), 9);
    }

    #[test]
    fn parse_id_rejects_garbage() {
        assert!(parse_id("foo").is_err());
        assert!(parse_id("T-").is_err());
        assert!(parse_id("").is_err());
    }

    #[test]
    fn parse_deps_none_returns_none() {
        assert_eq!(parse_deps(None).unwrap(), None);
    }

    #[test]
    fn parse_deps_empty_string_returns_none() {
        assert_eq!(parse_deps(Some("".into())).unwrap(), None);
        assert_eq!(parse_deps(Some("   ".into())).unwrap(), None);
    }

    #[test]
    fn parse_deps_comma_separated() {
        let got = parse_deps(Some("T-3,T-5,9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn parse_deps_whitespace_separated() {
        let got = parse_deps(Some("T-3 5 9".into())).unwrap();
        assert_eq!(got.as_deref(), Some("[3,5,9]"));
    }

    #[test]
    fn deps_from_db_none_is_empty() {
        assert!(deps_from_db(None).is_empty());
    }

    #[test]
    fn deps_from_db_parses_json_array() {
        assert_eq!(deps_from_db(Some("[1,2,3]".into())), vec![1, 2, 3]);
    }

    #[test]
    fn deps_from_db_invalid_json_is_empty() {
        assert_eq!(deps_from_db(Some("not-json".into())), Vec::<i64>::new());
    }
}
```

- [ ] **Step 2.2: Remove the moved items from `src/main.rs`**

In `src/main.rs`:

1. Add at the top, after the existing `use` lines: `mod model;`
2. Delete the `Task` struct (currently lines 149-161)
3. Delete the `Group` struct (currently lines 163-171)
4. Delete `now`, `fmt_id`, `parse_id`, `parse_deps`, `deps_from_db` (currently lines 175-217)
5. Delete the `#[cfg(test)] mod model_tests` block appended in Task 1 (it now lives in `model.rs`)
6. Replace usages: add `use model::{Task, Group, fmt_id, parse_id, parse_deps, deps_from_db, now};` near the existing `use` block at the top

Also remove now-unused imports `chrono::Utc`, `serde::Serialize`, `serde_json` from `main.rs` if they're no longer referenced — `cargo build` will warn.

- [ ] **Step 2.3: Compile and resolve warnings**

Run: `cargo build`
Expected: clean build. If there are unused-import warnings, delete the offending lines.

- [ ] **Step 2.4: Run full test suite**

Run: `cargo test`
Expected: 22 tests pass (11 integration + 11 unit, now under `model::tests`).

- [ ] **Step 2.5: Commit**

```bash
git add src/main.rs src/model.rs
git commit -m "refactor: extract model module (Task, Group, parse helpers)"
```

---

## Task 3: Extract `prompts.rs`

**Files:**
- Create: `src/prompts.rs`
- Modify: `src/main.rs` — declare module, remove moved items, update use-sites

- [ ] **Step 3.1: Create `src/prompts.rs`**

Write `src/prompts.rs`:

```rust
use crate::model::{fmt_id, Task};

pub const PROMPT_TDD: &str = include_str!("../templates/tdd-pursuit.md");
pub const PROMPT_CREATE_TASK: &str = include_str!("../templates/create-task.md");
pub const PROMPT_COMMIT: &str = include_str!("../templates/commit.md");
pub const PROMPT_PR: &str = include_str!("../templates/pr.md");

pub fn assemble_prompt(t: &Task) -> String {
    let body = t.body.as_deref().unwrap_or("(no body)");
    let acceptance = t.acceptance.as_deref().unwrap_or("(no acceptance criteria)");
    format!(
        "# Task {}: {}\n\n## Body\n{}\n\n## Acceptance\n{}\n\n{}",
        fmt_id(t.id),
        t.title,
        body,
        acceptance,
        PROMPT_TDD
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(body: Option<&str>, acceptance: Option<&str>) -> Task {
        Task {
            id: 7,
            title: "do thing".into(),
            body: body.map(String::from),
            acceptance: acceptance.map(String::from),
            deps: vec![],
            status: "open".into(),
            priority: 2,
            group: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn assemble_prompt_includes_title_and_id() {
        let s = assemble_prompt(&make_task(Some("b"), Some("a")));
        assert!(s.starts_with("# Task T-7: do thing"));
    }

    #[test]
    fn assemble_prompt_substitutes_missing_body() {
        let s = assemble_prompt(&make_task(None, Some("a")));
        assert!(s.contains("## Body\n(no body)"));
    }

    #[test]
    fn assemble_prompt_substitutes_missing_acceptance() {
        let s = assemble_prompt(&make_task(Some("b"), None));
        assert!(s.contains("## Acceptance\n(no acceptance criteria)"));
    }

    #[test]
    fn assemble_prompt_appends_tdd_template() {
        let s = assemble_prompt(&make_task(Some("b"), Some("a")));
        assert!(s.contains(PROMPT_TDD.trim_start()));
    }
}
```

- [ ] **Step 3.2: Remove moved items from `src/main.rs`**

In `src/main.rs`:

1. Add `mod prompts;` near the other `mod` declarations
2. Delete the four `const PROMPT_*` lines (currently lines 37-40)
3. Delete `fn assemble_prompt` (currently lines 570-581)
4. Add `use prompts::{assemble_prompt, PROMPT_COMMIT, PROMPT_CREATE_TASK, PROMPT_PR, PROMPT_TDD};` to the use-block

- [ ] **Step 3.3: Compile**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 3.4: Run full test suite**

Run: `cargo test`
Expected: 26 tests pass (11 integration + 11 model + 4 prompts).

- [ ] **Step 3.5: Commit**

```bash
git add src/main.rs src/prompts.rs
git commit -m "refactor: extract prompts module"
```

---

## Task 4: Extract `db.rs` (read-side only)

**Files:**
- Create: `src/db.rs`
- Modify: `src/main.rs` — declare module, remove moved items, update use-sites

Mutation helpers (`insert_task`, `update_task`, `delete_task`, `set_status`) land in **Task 5** — splitting the extract from the additions keeps each commit reviewable.

- [ ] **Step 4.1: Create `src/db.rs` with the read-side and schema**

Write `src/db.rs`:

```rust
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
        init_schema(&c).unwrap(); // running again should not error
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
}
```

- [ ] **Step 4.2: Remove moved items from `src/main.rs`**

In `src/main.rs`:

1. Add `mod db;` near the other `mod` declarations
2. Delete `const SCHEMA`, `find_repo_root`, `docket_dir`, `open_db`, `load_all_tasks`, `get_or_create_group` (the old definitions)
3. Add `use db::{find_repo_root, get_or_create_group, init_schema, load_all_groups, load_all_tasks, load_group_by_name, open_db};` to the use-block
4. In `cmd_init`, replace `conn.execute_batch(SCHEMA)?;` with `init_schema(&conn)?;`
5. In `cmd_group_show` and `cmd_group_new`, replace inline `query_row(...)` lookups against the `groups` table with `load_group_by_name(&conn, &name)?` where appropriate (only the read; the insert stays inline for now since it has unique fields)

- [ ] **Step 4.3: Compile and tidy unused imports**

Run: `cargo build`
Expected: clean build. Remove any newly unused imports (likely `rusqlite::OptionalExtension`, `Path`/`PathBuf` if no longer used in main.rs).

- [ ] **Step 4.4: Run full test suite**

Run: `cargo test`
Expected: 31 tests pass (11 integration + 11 model + 4 prompts + 5 db).

- [ ] **Step 4.5: Commit**

```bash
git add src/main.rs src/db.rs
git commit -m "refactor: extract db module (schema, connection, read-side queries)"
```

---

## Task 5: Add mutation helpers to `db.rs`

These helpers exist for the TUI but also let existing CLI commands stop building SQL strings inline. We migrate the CLI callers in the same task so the helpers have at least one in-tree consumer immediately.

**Files:**
- Modify: `src/db.rs` — add four mutation functions + tests
- Modify: `src/main.rs` — switch CLI commands to use them

- [ ] **Step 5.1: Add `insert_task` to `src/db.rs`**

Append to `src/db.rs` (before the `#[cfg(test)] mod tests` block):

```rust
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
```

- [ ] **Step 5.2: Write test for `insert_task`**

Inside `#[cfg(test)] mod tests` in `src/db.rs`, add:

```rust
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
```

- [ ] **Step 5.3: Run the new test**

Run: `cargo test db::tests::insert_task_returns_new_id_and_round_trips`
Expected: PASS.

- [ ] **Step 5.4: Add `set_status` and `delete_task` to `src/db.rs`**

Append:

```rust
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
```

- [ ] **Step 5.5: Write tests for `set_status` and `delete_task`**

Inside `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 5.6: Run the new tests**

Run: `cargo test db::tests`
Expected: all `db::tests` pass.

- [ ] **Step 5.7: Add `update_task` for TUI edits**

Append to `src/db.rs`:

```rust
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
pub fn update_task(conn: &Connection, id: i64, t: TaskUpdate) -> Result<usize> {
    let n = conn.execute(
        "UPDATE tasks SET title = ?1, body = ?2, acceptance = ?3, deps = ?4,
                          priority = ?5, group_id = ?6, updated_at = ?7
         WHERE id = ?8",
        params![t.title, t.body, t.acceptance, t.deps_json, t.priority, t.group_id, now(), id],
    )?;
    Ok(n)
}
```

- [ ] **Step 5.8: Write tests for `update_task`**

```rust
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
```

- [ ] **Step 5.9: Migrate `cmd_add`, `cmd_status`, `cmd_done`, `cmd_rm` in `src/main.rs`**

Replace the bodies of those functions to call the new `db::*` helpers:

```rust
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
```

Also remove the `use rusqlite::{params, ...}` items that are no longer used in `main.rs` (compiler will guide you).

- [ ] **Step 5.10: Run full test suite (regression check)**

Run: `cargo test`
Expected: all tests pass. Integration tests in `tests/start.rs` should still cover `cmd_add` / `cmd_done` / `cmd_rm` end-to-end and prove the migration didn't break behaviour.

- [ ] **Step 5.11: Commit**

```bash
git add src/db.rs src/main.rs
git commit -m "refactor: add db mutation helpers and migrate cli callers"
```

---

## Task 6: Create `src/cli/mod.rs` with clap structs and dispatcher

This task moves *only* the clap-derive structs and the dispatcher. Per-verb handlers stay in `main.rs` for now and migrate in Task 7. The split keeps each diff small.

**Files:**
- Create: `src/cli/mod.rs`
- Modify: `src/main.rs` — delete clap structs + dispatcher, call `cli::dispatch` from `main`

- [ ] **Step 6.1: Create `src/cli/mod.rs`**

Write `src/cli/mod.rs`:

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};

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
        Command::Init => crate::cmd_init(),
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
```

(Yes, this calls back into `crate::cmd_*` which still live in `main.rs`. Those move to per-verb files in Task 7, after which `dispatch` updates to call `cli::add::run`, etc.)

- [ ] **Step 6.2: Update `src/main.rs`**

1. Add `mod cli;` near the other `mod` declarations
2. Delete the `#[derive(Parser)] struct Cli`, `#[derive(Subcommand)] enum Command`, `#[derive(Subcommand)] enum GroupCommand` blocks
3. Delete the existing `match cli.command { ... }` body inside `fn main`
4. Make the `cmd_*` functions `pub(crate)` so `cli::dispatch` can call them (just add `pub(crate)` before each `fn cmd_*`)
5. Rewrite `fn main` to:

```rust
fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::dispatch(cli)
}
```

- [ ] **Step 6.3: Compile**

Run: `cargo build`
Expected: clean build. The `use clap::{Parser, Subcommand};` in `main.rs` becomes unused — delete it.

- [ ] **Step 6.4: Run full test suite**

Run: `cargo test`
Expected: all 39+ tests still pass.

- [ ] **Step 6.5: Commit**

```bash
git add src/main.rs src/cli/mod.rs
git commit -m "refactor: extract clap structs and dispatcher into cli module"
```

---

## Task 7: Move each verb handler into `src/cli/<verb>.rs`

One commit per verb keeps each move trivial to review. The pattern is identical for every verb, so the steps below walk through `init.rs` in detail and abbreviate the rest.

**Files:**
- Create: `src/cli/init.rs`, `src/cli/add.rs`, `src/cli/ls.rs`, `src/cli/show.rs`, `src/cli/ready.rs`, `src/cli/blocked.rs`, `src/cli/status.rs`, `src/cli/rm.rs`, `src/cli/prompt.rs`, `src/cli/start.rs`, `src/cli/group.rs`
- Modify: `src/cli/mod.rs` — declare each submodule, switch `dispatch` to call them
- Modify: `src/main.rs` — delete each `cmd_*` after it moves

### Task 7a: Move `cmd_init`

- [ ] **Step 7a.1: Create `src/cli/init.rs`**

```rust
use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::fs;

use crate::db::{find_repo_root, init_schema};

pub fn run() -> Result<()> {
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
```

- [ ] **Step 7a.2: Wire it through `src/cli/mod.rs`**

Add at top of `src/cli/mod.rs`: `pub mod init;`
Change the dispatch arm: `Command::Init => init::run(),`

- [ ] **Step 7a.3: Delete `cmd_init` from `src/main.rs`**

Remove the entire `pub(crate) fn cmd_init() -> Result<()> { ... }` block.

- [ ] **Step 7a.4: Compile + test**

Run: `cargo test`
Expected: all tests still pass (integration tests cover `docket init`).

- [ ] **Step 7a.5: Commit**

```bash
git add src/main.rs src/cli/mod.rs src/cli/init.rs
git commit -m "refactor: move init handler to src/cli/init.rs"
```

### Task 7b: Move `cmd_add`

- [ ] **Step 7b.1: Create `src/cli/add.rs`**

```rust
use anyhow::Result;

use crate::db::{self, get_or_create_group, open_db, NewTask};
use crate::model::{fmt_id, parse_deps};

pub fn run(
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
        NewTask {
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
```

- [ ] **Step 7b.2: Wire through `src/cli/mod.rs`** — add `pub mod add;`, change dispatch arm to `add::run(title, body, acceptance, deps, priority, group)`.

- [ ] **Step 7b.3: Delete `cmd_add` from `src/main.rs`**.

- [ ] **Step 7b.4: Run `cargo test`** — expect all pass.

- [ ] **Step 7b.5: Commit**: `refactor: move add handler to src/cli/add.rs`

### Task 7c: Move `cmd_ls`

- [ ] **Step 7c.1: Create `src/cli/ls.rs`**

```rust
use anyhow::Result;

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, Task};

fn print_row(t: &Task) {
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

pub fn run(status: Option<String>, group: Option<String>, json: bool) -> Result<()> {
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
            print_row(t);
        }
    }
    Ok(())
}
```

Note: `print_task_row` was a free function shared by `cmd_ls`, `cmd_ready`, `cmd_blocked`, `cmd_group_show`. Each verb gets its own copy of the helper to keep the move trivial — DRY can be revisited later by lifting the helper into `src/cli/mod.rs` (do it inside Task 7g below, where it makes sense).

- [ ] **Step 7c.2: Wire through `src/cli/mod.rs`** — `pub mod ls;`, dispatch arm: `ls::run(status, group, json)`.

- [ ] **Step 7c.3: Delete `cmd_ls` from `src/main.rs`** (keep `print_task_row` for now — it's still used by other verbs).

- [ ] **Step 7c.4: Run `cargo test`** — expect all pass.

- [ ] **Step 7c.5: Commit**: `refactor: move ls handler to src/cli/ls.rs`

### Task 7d: Move `cmd_show`

- [ ] **Step 7d.1: Create `src/cli/show.rs`**

```rust
use anyhow::{anyhow, Result};

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String, json: bool) -> Result<()> {
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
```

- [ ] **Step 7d.2: Wire, delete, test, commit** as in prior subtasks. Commit message: `refactor: move show handler to src/cli/show.rs`

### Task 7e: Move `cmd_ready`

- [ ] **Step 7e.1: Create `src/cli/ready.rs`**

```rust
use anyhow::Result;
use std::collections::HashMap;

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, Task};

fn print_row(t: &Task) {
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

pub fn run(group: Option<String>, json: bool) -> Result<()> {
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
            print_row(t);
        }
    }
    Ok(())
}
```

- [ ] **Step 7e.2: Wire, delete `cmd_ready` from main, test, commit.** Message: `refactor: move ready handler to src/cli/ready.rs`

### Task 7f: Move `cmd_blocked`

- [ ] **Step 7f.1: Create `src/cli/blocked.rs`**

```rust
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;

use crate::db::{load_all_tasks, open_db};
use crate::model::{fmt_id, Task};

fn print_row(t: &Task) {
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

#[derive(Serialize)]
struct BlockedEntry {
    task: Task,
    unmet_deps: Vec<String>,
}

pub fn run(group: Option<String>, json: bool) -> Result<()> {
    let conn = open_db()?;
    let all = load_all_tasks(&conn)?;
    let by_id: HashMap<i64, &Task> = all.iter().map(|t| (t.id, t)).collect();

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
            print_row(&e.task);
            println!("       blocked by: {}", e.unmet_deps.join(", "));
        }
    }
    Ok(())
}
```

- [ ] **Step 7f.2: Wire, delete, test, commit.** Message: `refactor: move blocked handler to src/cli/blocked.rs`

### Task 7g: Lift the `print_row` helper

By now `print_row` is duplicated across `ls.rs`, `ready.rs`, `blocked.rs`, and (after 7l) `group.rs`. Lift it.

- [ ] **Step 7g.1: Add `print_task_row` to `src/cli/mod.rs`**

```rust
use crate::model::{fmt_id, Task};

pub(crate) fn print_task_row(t: &Task) {
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
```

- [ ] **Step 7g.2: Replace local `print_row` in each cli submodule with `super::print_task_row` (in `ls.rs`, `ready.rs`, `blocked.rs`)** and delete the local copy.

- [ ] **Step 7g.3: Run `cargo test`** — expect all pass.

- [ ] **Step 7g.4: Commit**: `refactor: lift print_task_row helper into cli::mod`

### Task 7h: Move `cmd_status` and `cmd_done`

- [ ] **Step 7h.1: Create `src/cli/status.rs`**

```rust
use anyhow::{anyhow, Result};

use crate::db::{open_db, set_status};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String, state: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = set_status(&conn, id, &state)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} -> {}", fmt_id(id), state);
    Ok(())
}

pub fn done(id: String) -> Result<()> {
    run(id, "done".into())
}
```

- [ ] **Step 7h.2: Wire `Command::Status` to `status::run` and `Command::Done` to `status::done` in `src/cli/mod.rs`.** Add `pub mod status;`.

- [ ] **Step 7h.3: Delete `cmd_status` and `cmd_done` from `main.rs`. Run `cargo test`. Commit.** Message: `refactor: move status/done handlers to src/cli/status.rs`

### Task 7i: Move `cmd_rm`

- [ ] **Step 7i.1: Create `src/cli/rm.rs`**

```rust
use anyhow::{anyhow, Result};

use crate::db::{delete_task, open_db};
use crate::model::{fmt_id, parse_id};

pub fn run(id: String) -> Result<()> {
    let id = parse_id(&id)?;
    let conn = open_db()?;
    let n = delete_task(&conn, id)?;
    if n == 0 {
        return Err(anyhow!("{} not found", fmt_id(id)));
    }
    println!("{} deleted", fmt_id(id));
    Ok(())
}
```

- [ ] **Step 7i.2: Wire, delete `cmd_rm`, test, commit.** Message: `refactor: move rm handler to src/cli/rm.rs`

### Task 7j: Move `cmd_prompt`

- [ ] **Step 7j.1: Create `src/cli/prompt.rs`**

```rust
use anyhow::{anyhow, Result};

use crate::prompts::{PROMPT_COMMIT, PROMPT_CREATE_TASK, PROMPT_PR, PROMPT_TDD};

pub fn run(name: String) -> Result<()> {
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
```

- [ ] **Step 7j.2: Wire, delete `cmd_prompt`, test, commit.** Message: `refactor: move prompt handler to src/cli/prompt.rs`

### Task 7k: Move `cmd_start` and `deliver_via_tmux`

- [ ] **Step 7k.1: Create `src/cli/start.rs`**

```rust
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use rusqlite::Connection;
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
        rusqlite::params![now(), id],
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
```

- [ ] **Step 7k.2: Wire, delete `cmd_start` + `mark_in_progress` + `deliver_via_tmux` from `main.rs`, test, commit.** Message: `refactor: move start handler to src/cli/start.rs`

### Task 7l: Move `cmd_group_*`

- [ ] **Step 7l.1: Create `src/cli/group.rs`**

```rust
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::Serialize;

use crate::cli::print_task_row;
use crate::db::{load_all_groups, load_all_tasks, load_group_by_name, open_db};
use crate::model::{now, Group, Task};

pub fn new(name: String, branch: Option<String>, description: Option<String>) -> Result<()> {
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

pub fn ls(json: bool) -> Result<()> {
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

pub fn show(name: String, json: bool) -> Result<()> {
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

pub fn close(name: String) -> Result<()> {
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
```

- [ ] **Step 7l.2: Wire** `GroupCommand::*` arms in `src/cli/mod.rs` to `group::new` / `group::ls` / `group::show` / `group::close`. Add `pub mod group;`.

- [ ] **Step 7l.3: Delete `cmd_group_*` from `main.rs`. Run `cargo test`. Commit.** Message: `refactor: move group handlers to src/cli/group.rs`

---

## Task 8: Final cleanup of `src/main.rs`

After every verb moves, `src/main.rs` should be roughly 25 lines.

- [ ] **Step 8.1: Verify final shape**

Open `src/main.rs`. It should look approximately like:

```rust
use anyhow::Result;

mod cli;
mod db;
mod model;
mod prompts;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::dispatch(cli)
}
```

If anything else is still in `main.rs` (stray helpers, `print_task_row`, unused `use` statements), delete it. The compiler should agree there are no unused items.

- [ ] **Step 8.2: Add `Cli` and `Parser` import**

The snippet above won't compile as written because `Cli::parse` needs `clap::Parser` in scope. Adjust:

```rust
use anyhow::Result;
use clap::Parser;

mod cli;
mod db;
mod model;
mod prompts;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    cli::dispatch(cli)
}
```

- [ ] **Step 8.3: Run the full suite one more time**

Run: `cargo test`
Expected: all tests pass (11 integration + 11 model + 4 prompts + 13 db = 39).

- [ ] **Step 8.4: Run `cargo build --release` to confirm the release profile still compiles**

Run: `cargo build --release`
Expected: clean build.

- [ ] **Step 8.5: Run the binary end-to-end as a smoke test**

```bash
cd $(mktemp -d) && git init -q
"$OLDPWD"/target/release/docket init
"$OLDPWD"/target/release/docket add "smoke" --body "b" --acceptance "a"
"$OLDPWD"/target/release/docket ls
"$OLDPWD"/target/release/docket show T-1
"$OLDPWD"/target/release/docket ready
"$OLDPWD"/target/release/docket done T-1
"$OLDPWD"/target/release/docket ls --status done
```

Expected: each command behaves identically to pre-refactor.

- [ ] **Step 8.6: Commit if `main.rs` changed in step 8.1**

If step 8.1 deleted leftover code:

```bash
git add src/main.rs
git commit -m "refactor: shrink main.rs to a thin dispatcher"
```

Otherwise skip.

---

## Self-review (run mentally before handing off)

**Spec coverage:**
- §3.1 module layout — Tasks 2–7 produce each named file
- §3.4 state ownership / mutation helpers — Task 5 lands `insert_task`, `update_task`, `set_status`, `delete_task`
- §8 testing — Tasks 1, 4, 5 add unit tests on `model`, `prompts`, `db`; existing `tests/start.rs` continues to gate behaviour
- §9 sequencing — this plan IS step 1 of the spec's sequence

**Placeholder scan:** no TBDs, no "implement later", every step has the exact code or command to run.

**Type consistency:** `NewTask` and `TaskUpdate` field names and signatures stay identical across `db.rs` and the `cli::add` / future `cli::edit` callers. `print_task_row` signature is consistent across `cli/mod.rs` and consumers.

**Out of scope for this plan (intentionally):** TUI itself (PR-2+), new dependencies, the `tui` clap subcommand, themes. Those land in subsequent plans in this same series.
