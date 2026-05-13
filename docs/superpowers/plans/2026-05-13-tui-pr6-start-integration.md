# Docket TUI — PR-6: Start Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From the list pane, `S` exits the TUI and runs `docket start T-N` (prints the prompt and marks the task `in_progress`); `Ctrl+S` exits and runs `docket start T-N --tmux`. Both reuse the existing `cli::start::run` so the prompt assembly, status-update, and tmux delivery code paths stay single-sourced.

**Architecture:** `App` gains `pending_start: Option<StartRequest>`. When `S` or `Ctrl+S` is pressed in the list scope, we populate `pending_start` and set `should_quit`. The event loop in `tui::mod` breaks out, the `TerminalGuard` tears down raw mode + alt-screen on drop, and `run_tui` returns `Option<StartRequest>` to its caller. `cli::tui::run` then calls `cli::start::run(fmt_id(id), tmux)`, whose stdout / tmux call now lands on a normal terminal. No new DB code: `cli::start::run` opens its own connection.

**Tech Stack:** unchanged from PR-5.

---

## File Structure (delta from PR-5)

```
src/tui/
  app.rs               # Modify: add StartRequest type and App::pending_start;
                       #   handle S / Ctrl+S in handle_list
  mod.rs               # Modify: run_tui returns Result<Option<StartRequest>>
src/cli/
  tui.rs               # Modify: after run_tui returns Some(req), call
                       #   start::run(fmt_id(req.id), req.tmux)
```

---

## Task 1: Add `StartRequest`; thread it through `run_tui` and `cli::tui::run`

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/mod.rs`
- Modify: `src/cli/tui.rs`

- [ ] **Step 1.1: Add `StartRequest` and `pending_start` to App**

In `src/tui/app.rs`, add this type near the top (after the `Pane` enum):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartRequest {
    pub id: i64,
    pub tmux: bool,
}
```

In the `App` struct definition, add the field (right after `should_quit`):

```rust
pub struct App {
    pub conn: Connection,
    pub tasks: Vec<Task>,
    pub filters: Filters,
    pub cursor: usize,
    pub focus: Pane,
    pub screen: Screen,
    pub pending_chord: Option<char>,
    pub should_quit: bool,
    pub pending_start: Option<StartRequest>,
}
```

In `App::new`, initialise it:

```rust
        Ok(Self {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
            pending_start: None,
        })
```

In the `mem_app` test helper (inside `mod tests`), initialise it too:

```rust
        App {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
            pending_start: None,
        }
```

- [ ] **Step 1.2: Update `run_tui` to return the pending start**

Replace the body of `pub fn run_tui()` in `src/tui/mod.rs`:

```rust
pub fn run_tui() -> Result<Option<app::StartRequest>> {
    let mut app = App::new()?;
    let mut guard = TerminalGuard::new()?;

    loop {
        guard.terminal().draw(|f| app.render(f))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(app.pending_start)
}
```

- [ ] **Step 1.3: Update `cli::tui::run` to dispatch to start**

Replace `src/cli/tui.rs`:

```rust
use anyhow::Result;

use crate::model::fmt_id;

pub fn run() -> Result<()> {
    if let Some(req) = crate::tui::run_tui()? {
        crate::cli::start::run(fmt_id(req.id), req.tmux)?;
    }
    Ok(())
}
```

- [ ] **Step 1.4: Build and run the suite**

Run: `cargo build` — clean (one expected dead_code warning on `StartRequest` if nothing consumes it yet; we'll add `#[allow(dead_code)]` if needed).

If a dead_code warning fires on the `StartRequest` fields, prefix the struct:

```rust
#[allow(dead_code)] // consumed by cli::tui::run via app.pending_start
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartRequest {
    pub id: i64,
    pub tmux: bool,
}
```

Run: `cargo test --quiet` — expected: 139 tests pass (no behaviour change yet).

- [ ] **Step 1.5: Commit**

```bash
git add src/tui/app.rs src/tui/mod.rs src/cli/tui.rs
git commit -m "feat(tui): plumb StartRequest through run_tui to cli::start"
```

---

## Task 2: `S` (capital) starts the cursor task and exits

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 2.1: Write failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn capital_s_sets_pending_start_no_tmux_and_quits() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key_with(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert!(app.should_quit);
        assert_eq!(
            app.pending_start,
            Some(StartRequest {
                id,
                tmux: false
            })
        );
    }

    #[test]
    fn capital_s_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert!(!app.should_quit);
        assert!(app.pending_start.is_none());
    }
```

(`StartRequest` is in scope via `use super::*;`.)

- [ ] **Step 2.2: Run to confirm failures**

Run: `cargo test tui::app::tests::capital_s_`

Expected: 1 FAIL (the noop case is trivially true; the active case fails).

- [ ] **Step 2.3: Implement `S`**

In `handle_list`, add the `Char('S')` arm after `Char('e')`:

```rust
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            KeyCode::Char('S') => {
                self.request_start(false);
            }
            _ => {}
        }
    }
```

Add the `request_start` method to `impl App`, immediately after `open_edit_form`:

```rust
    fn request_start(&mut self, tmux: bool) {
        if let Some(task) = self.selected_task() {
            self.pending_start = Some(StartRequest {
                id: task.id,
                tmux,
            });
            self.should_quit = true;
        }
    }
```

- [ ] **Step 2.4: Run tests**

Run: `cargo test tui::app::tests::capital_s_`

Expected: 2 PASS.

- [ ] **Step 2.5: Run full suite**

Run: `cargo test --quiet`

Expected: 141 tests pass.

- [ ] **Step 2.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): S starts the cursor task (parity with docket start T-N)"
```

---

## Task 3: `Ctrl+S` starts the cursor task with tmux delivery

**Files:**
- Modify: `src/tui/app.rs`

Note: `Ctrl+S` in the **Edit form** scope already binds to "save" (PR-5 Task 8). That binding only fires while `Screen::Edit` is active. List-scope `Ctrl+S` lands in `handle_list`, which is only reached when `Screen::Main`. So the bindings do not conflict at runtime.

- [ ] **Step 3.1: Write failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn ctrl_s_in_list_sets_pending_start_with_tmux_and_quits() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
        assert_eq!(
            app.pending_start,
            Some(StartRequest {
                id,
                tmux: true
            })
        );
    }

    #[test]
    fn ctrl_s_in_list_on_empty_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!app.should_quit);
        assert!(app.pending_start.is_none());
    }
```

- [ ] **Step 3.2: Run to confirm failures**

Run: `cargo test tui::app::tests::ctrl_s_in_list_`

Expected: 1 FAIL.

- [ ] **Step 3.3: Implement Ctrl+S in `handle_main`**

`Ctrl+S` does NOT route through `handle_list` because the existing `handle_main` matches `(KeyCode::Char(c), modifiers)` against global keys first and then forwards to pane-scoped handlers — but the pane-scoped handlers receive the raw KeyEvent including modifiers. The simplest path: add a list-only branch in `handle_main` for Ctrl+S, before falling through to `handle_list`.

In `src/tui/app.rs`, modify `handle_main` — add the new arm to the existing `match (key.code, key.modifiers)` block, immediately after the existing `(KeyCode::Char('R'), _)` arm:

```rust
            (KeyCode::Char('R'), _) => {
                let _ = self.reload();
                return;
            }
            (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                // List-scope only — start with tmux delivery.
                if self.focus == Pane::List {
                    self.request_start(true);
                }
                return;
            }
            _ => {}
        }
```

(We gate on `focus == Pane::List` to mirror spec §4.2, which lists `Ctrl+s` under the list scope. Detail scope is unaffected.)

- [ ] **Step 3.4: Run tests**

Run: `cargo test tui::app::tests::ctrl_s_in_list_`

Expected: 2 PASS.

- [ ] **Step 3.5: Run full suite**

Run: `cargo test --quiet`

Expected: 143 tests pass. All previously-green tests stay green — including the Edit-form `ctrl_s_save_*` tests, because those fire only when `Screen::Edit` is active (which routes through `handle_edit`, not `handle_main`).

- [ ] **Step 3.6: Verify warning-free build**

Run: `cargo build 2>&1 | grep -i warning || echo "no warnings"`

Expected: `no warnings`.

- [ ] **Step 3.7: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): Ctrl+S starts cursor task with tmux delivery"
```

---

## Task 4: Update keybindings registry + footer

**Files:**
- Modify: `src/tui/keybindings.rs`

The bindings should appear in the help reference and (for `S`) the list-scope footer. `Ctrl+S` is rare enough that we leave it off the footer to avoid clutter; help shows it.

- [ ] **Step 4.1: Add the bindings**

In `src/tui/keybindings.rs`, add two entries to the `COMMAND_REGISTRY` — just after the existing `Binding { keys: "x", label: "delete", ... }` line:

```rust
    Binding { keys: "x", label: "delete", scope: Scope::List, footer: true },
    Binding { keys: "S", label: "start", scope: Scope::List, footer: true },
    Binding { keys: "Ctrl+S", label: "start (tmux)", scope: Scope::List, footer: false },
```

- [ ] **Step 4.2: Add a registry test**

Append to the `tests` module in `src/tui/keybindings.rs`:

```rust
    #[test]
    fn registry_contains_start_keys() {
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "S" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "Ctrl+S" && b.scope == Scope::List));
    }
```

- [ ] **Step 4.3: Run tests**

Run: `cargo test --quiet`

Expected: 144 tests pass.

- [ ] **Step 4.4: Commit**

```bash
git add src/tui/keybindings.rs
git commit -m "feat(tui): registry entries for S and Ctrl+S start"
```

---

## Task 5: Manual smoke test (no commit)

- [ ] **Step 5.1: Build**

Run: `cargo build --quiet` — clean.

- [ ] **Step 5.2: CLI surface check**

Run: `cargo run --quiet -- tui --help` — clap help prints.

- [ ] **Step 5.3: Hand off**

Report: PR-6 complete. 5 commits on `tui-design-spec` (4 feature + 1 plan). All six PRs landed; the TUI is feature-complete per spec §1.

For visual verification: launch with `cargo run -- tui`, place the cursor on a task, press `S` — TUI should exit and the prompt should appear in the terminal with the task marked `in_progress`. From a tmux session, press `Ctrl+S` instead — a new tmux window opens with `claude < <tempfile>` running.

---

## Push step

```bash
git push origin tui-design-spec
```

After verification, the user can decide whether to open a GitHub PR for the branch as a whole.

---

## Self-Review

**Spec coverage** (against §4.2 + §4.3):

- §4.2 list-scope `S` start task — Task 2 + Task 4 (registry)
- §4.2 list-scope `Ctrl+s` start with tmux — Task 3 + Task 4 (registry)
- §4.3 `S` exits the TUI after printing the assembled prompt — Task 2 (request_start sets `should_quit`); Task 1 (run_tui returns the StartRequest; cli::tui::run calls cli::start::run which prints + marks in_progress)
- §4.3 status-line for "not inside a tmux session" — already inherited from `cli::start::run`, which returns the same error. The TUI is already gone by that point so the error surfaces on the normal terminal, identical to `docket start T-N --tmux` outside tmux today.

**Placeholder scan:** all steps concrete; no TBDs.

**Type consistency:**
- `StartRequest { id: i64, tmux: bool }` — defined in Task 1; constructed in Task 2 / Task 3 (`request_start`); consumed in Task 1 (`cli::tui::run` reads `req.id` and `req.tmux`).
- `request_start(tmux: bool)` — defined in Task 2; called from Task 2 (`S`) and Task 3 (`Ctrl+S` arm).
- `App::pending_start: Option<StartRequest>` — populated by handlers, read by `run_tui` at exit. The field is `pub` so `run_tui` (in the parent module) can read it without an accessor.

**Atomic-commit discipline:**
- Task 1 ships scaffolding that compiles + leaves all tests green; no behaviour change.
- Tasks 2 / 3 / 4 each ship a single feature + its tests; each commit is independently green.

**Test count progression:** 139 → 139 (Task 1, no behavioural change) → 141 (Task 2 +2) → 143 (Task 3 +2) → 144 (Task 4 +1).

**Out of scope:**
- Showing the in-TUI confirmation/status line for the in-progress flag flip — handled by `cli::start::run` after teardown.
- Backward cycle of status (`Shift+S`) — permanently deferred since the key collides with `S = start` (documented in PR-4 plan).
