# Docket TUI — PR-4: Mutations (Status / Done / Delete) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three mutating list-scope actions to the TUI — `s` cycles task status forward, `d` marks the cursor task done, and `x` opens a Confirm modal that deletes the task on `y`/`Enter` and cancels on `n`/`Esc`. No add/edit form yet (PR-5), no start integration yet (PR-6).

**Architecture:** Each list-scope mutation goes through an existing `db::*` function (`set_status` / `delete_task`), then calls `App::reload()` to refetch tasks. The `Screen` enum grows a `Confirm(PendingAction)` variant; modal events route to a new `handle_confirm` before reaching list/detail handlers (mirrors the existing Help / FilterPrompt dispatch). Status cycling and mark-done preserve the cursor on the affected task by id (so the user keeps watching the row they just acted on). Delete clamps to the visible range.

**Tech Stack:** ratatui 0.29, crossterm 0.28, rusqlite. No new deps.

**Spec resolution:** §4.2 binds `s` / `Shift+s` to "cycle status forward/backward" *and* `S` to "start task" — but `Shift+s` and `S` are the same key. Resolution: PR-4 implements `s` (forward) only. Backward cycle is dropped in favour of `S` = start in PR-6 (the canonical cycle has only three states, so re-pressing `s` twice walks the cycle backward in two keystrokes).

---

## File Structure (delta from PR-3)

```
src/tui/
  screens/
    mod.rs          # Modify: add PendingAction + Screen::Confirm(PendingAction)
    confirm.rs      # Create: render confirm modal
  app.rs            # Modify: handle s/d/x in handle_list; add handle_confirm;
                    #   add next_status() helper; add preserve_cursor_on(id)
                    #   helper; route Confirm screen in handle_key + render
  keybindings.rs    # Modify: add Scope::Confirm; bindings for s/d/x and confirm
```

**Boundaries:**
- `PendingAction` lives in `screens/mod.rs` next to `Screen` — it's data the screen state machine owns.
- `next_status` and `preserve_cursor_on` are private fns on `App` in `app.rs`.
- The confirm renderer in `screens/confirm.rs` takes a `&PendingAction` and is pure — no `&App` needed.
- The Confirm modal is responsible *only* for the destructive `DeleteTask` action this PR. PR-5 will add `DiscardEdits` for the edit-form dirty-state confirm.

---

## Task 1: Extend Screen with Confirm + PendingAction

**Files:**
- Modify: `src/tui/screens/mod.rs`

- [ ] **Step 1.1: Add `PendingAction` and `Screen::Confirm` variant**

Replace the `Screen` enum block at the top of `src/tui/screens/mod.rs` (keep `pub mod` declarations and `FilterKind` unchanged):

```rust
pub mod filter_prompt;
pub mod help;
pub mod main;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterKind {
    Status,
    Group,
    Priority,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    DeleteTask { id: i64, title: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
    Confirm(PendingAction),
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
```

- [ ] **Step 1.2: Compile**

Run: `cargo build`

Expected: FAIL — `App::handle_key` and `App::render` `match` on `&self.screen` is no longer exhaustive (missing `Screen::Confirm` arm). This is intentional; Task 3 wires it up. **Do not commit yet** — paired with Task 2.

---

## Task 2: Add `Scope::Confirm` and registry bindings

**Files:**
- Modify: `src/tui/keybindings.rs`

- [ ] **Step 2.1: Add the Confirm scope and new bindings**

Replace the `Scope` enum and `COMMAND_REGISTRY` in `src/tui/keybindings.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    List,
    Detail,
    Help,
    FilterPrompt,
    Confirm,
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static str,
    pub label: &'static str,
    pub scope: Scope,
    pub footer: bool,
}

pub const COMMAND_REGISTRY: &[Binding] = &[
    Binding { keys: "?", label: "help", scope: Scope::Global, footer: true },
    Binding { keys: "q", label: "quit", scope: Scope::Global, footer: true },
    Binding { keys: "/", label: "filter text", scope: Scope::Global, footer: true },
    Binding { keys: "R", label: "reload", scope: Scope::Global, footer: false },
    Binding { keys: "j/k", label: "nav", scope: Scope::List, footer: true },
    Binding { keys: "g/G", label: "top/bottom", scope: Scope::List, footer: false },
    Binding { keys: "l", label: "detail", scope: Scope::List, footer: true },
    Binding { keys: "s", label: "cycle status", scope: Scope::List, footer: true },
    Binding { keys: "d", label: "done", scope: Scope::List, footer: true },
    Binding { keys: "x", label: "delete", scope: Scope::List, footer: true },
    Binding { keys: "f s", label: "filter status", scope: Scope::List, footer: false },
    Binding { keys: "f g", label: "filter group", scope: Scope::List, footer: false },
    Binding { keys: "f p", label: "filter priority", scope: Scope::List, footer: false },
    Binding { keys: "f r", label: "ready", scope: Scope::List, footer: true },
    Binding { keys: "f b", label: "blocked", scope: Scope::List, footer: false },
    Binding { keys: "f c", label: "clear filters", scope: Scope::List, footer: true },
    Binding { keys: "h", label: "list", scope: Scope::Detail, footer: true },
    Binding { keys: "PgUp/PgDn", label: "scroll", scope: Scope::Detail, footer: true },
    Binding { keys: "?/Esc", label: "close help", scope: Scope::Help, footer: true },
    Binding { keys: "Enter", label: "apply", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "y/Enter", label: "confirm", scope: Scope::Confirm, footer: true },
    Binding { keys: "n/Esc", label: "cancel", scope: Scope::Confirm, footer: true },
];

pub fn footer_for(scope: Scope) -> Vec<Binding> {
    COMMAND_REGISTRY
        .iter()
        .filter(|b| b.footer && (b.scope == Scope::Global || b.scope == scope))
        .copied()
        .collect()
}
```

- [ ] **Step 2.2: Update keybindings tests**

Replace the `#[cfg(test)] mod tests` block at the bottom of `src/tui/keybindings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_quit_and_help() {
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "q"));
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "?"));
    }

    #[test]
    fn registry_contains_mutation_keys() {
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "s" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "d" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "x" && b.scope == Scope::List));
    }

    #[test]
    fn footer_for_list_includes_globals_and_list_keys() {
        let fb = footer_for(Scope::List);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"quit"));
        assert!(labels.contains(&"help"));
        assert!(labels.contains(&"nav"));
        assert!(labels.contains(&"detail"));
        assert!(labels.contains(&"clear filters"));
        assert!(labels.contains(&"cycle status"));
        assert!(labels.contains(&"done"));
        assert!(labels.contains(&"delete"));
    }

    #[test]
    fn footer_for_detail_includes_globals_and_detail_keys() {
        let fb = footer_for(Scope::Detail);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"quit"));
        assert!(labels.contains(&"list"));
        assert!(labels.contains(&"scroll"));
    }

    #[test]
    fn footer_for_filter_prompt_includes_apply_and_cancel() {
        let fb = footer_for(Scope::FilterPrompt);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"apply"));
        assert!(labels.contains(&"cancel"));
    }

    #[test]
    fn footer_for_confirm_includes_confirm_and_cancel() {
        let fb = footer_for(Scope::Confirm);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"confirm"));
        assert!(labels.contains(&"cancel"));
    }
}
```

- [ ] **Step 2.3: Compile**

Run: `cargo build`

Expected: still FAILS on the non-exhaustive `Screen` match in `app.rs` (Task 3 fixes). Do not commit yet.

---

## Task 3: Wire Confirm into App::handle_key and App::render (no behaviour change yet)

**Files:**
- Modify: `src/tui/app.rs`

This task makes Tasks 1 + 2 + 3 commit as a single atomic scaffolding commit. After this task the build is clean and 77 tests still pass; `Confirm` exists but is unreachable from the UI.

- [ ] **Step 3.1: Update `App::handle_key` to route Confirm**

In `src/tui/app.rs`, replace the body of `handle_key`:

```rust
    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.screen {
            Screen::Main => self.handle_main(key),
            Screen::Help => self.handle_help(key),
            Screen::FilterPrompt { .. } => self.handle_filter_prompt(key),
            Screen::Confirm(_) => self.handle_confirm(key),
        }
    }
```

- [ ] **Step 3.2: Add a stub `handle_confirm`**

Add the following method to the `impl App` block in `src/tui/app.rs`, immediately after `handle_filter_prompt`:

```rust
    fn handle_confirm(&mut self, _key: KeyEvent) {
        // Real handler lands in Task 6.
    }
```

- [ ] **Step 3.3: Update `App::render` to dispatch Confirm**

Replace the `render` method in `src/tui/app.rs`:

```rust
    pub fn render(&self, frame: &mut Frame) {
        crate::tui::screens::main::render(self, frame);
        match &self.screen {
            Screen::Help => crate::tui::screens::help::render(frame),
            Screen::FilterPrompt { kind, input } => {
                crate::tui::screens::filter_prompt::render(frame, kind, input)
            }
            Screen::Confirm(action) => crate::tui::screens::confirm::render(frame, action),
            Screen::Main => {}
        }
    }
```

- [ ] **Step 3.4: Import `PendingAction` in app.rs**

Replace the `use crate::tui::screens::{FilterKind, Screen};` line near the top of `src/tui/app.rs` with:

```rust
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

(`PendingAction` will be used by handlers in later tasks; allow the unused import for now.)

- [ ] **Step 3.5: Create the confirm renderer module stub**

Create `src/tui/screens/confirm.rs`:

```rust
use ratatui::Frame;

use crate::tui::screens::PendingAction;

pub fn render(_frame: &mut Frame, _action: &PendingAction) {
    // Real renderer lands in Task 7.
}
```

- [ ] **Step 3.6: Declare the new submodule**

Update `src/tui/screens/mod.rs` — change the first three lines from:

```rust
pub mod filter_prompt;
pub mod help;
pub mod main;
```

to:

```rust
pub mod confirm;
pub mod filter_prompt;
pub mod help;
pub mod main;
```

- [ ] **Step 3.7: Silence the `PendingAction` import until Task 4 uses it**

Since `PendingAction` is imported but not yet referenced inside `app.rs` (its first use is in Task 5), add `#[allow(unused_imports)]` on the import line. In `src/tui/app.rs`, replace:

```rust
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

with:

```rust
#[allow(unused_imports)] // PendingAction is referenced in Task 5
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

(The `#[allow(unused_imports)]` will be removed at the end of Task 5.)

- [ ] **Step 3.8: Build and test**

Run: `cargo build` — expected: clean build, no warnings.
Run: `cargo test` — expected: 78 tests pass (77 prior + 1 new `footer_for_confirm_includes_confirm_and_cancel` + 1 new `registry_contains_mutation_keys` = 79). Actually expect **79** total.

If any prior test fails, stop and investigate before continuing.

- [ ] **Step 3.9: Commit Tasks 1+2+3 together**

```bash
git add src/tui/screens/mod.rs src/tui/screens/confirm.rs src/tui/keybindings.rs src/tui/app.rs
git commit -m "feat(tui): scaffold Confirm screen and mutation keybindings"
```

---

## Task 4: `s` cycles status forward on the cursor task

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 4.1: Write the failing test**

Append to the `tests` module at the bottom of `src/tui/app.rs` (just before the closing `}`):

```rust
    #[test]
    fn s_cycles_open_to_in_progress() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "in_progress");
    }

    #[test]
    fn s_cycles_in_progress_to_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn s_cycles_done_back_to_open() {
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "open");
    }

    #[test]
    fn s_cycles_custom_status_to_open() {
        let mut app = mem_app_with(&[("a", "wontfix", 2)]);
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().status, "open");
    }

    #[test]
    fn s_keeps_cursor_on_same_task_id_after_status_change() {
        let mut app = mem_app_with(&[
            ("first",  "open", 2),
            ("second", "open", 2),
            ("third",  "open", 2),
        ]);
        // Move cursor to the middle task.
        app.handle_key(key(KeyCode::Char('j')));
        let id_before = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('s')));
        assert_eq!(app.selected_task().unwrap().id, id_before);
        assert_eq!(app.selected_task().unwrap().status, "in_progress");
    }

    #[test]
    fn s_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('s')));
        assert!(app.selected_task().is_none());
    }
```

- [ ] **Step 4.2: Run tests to confirm they fail**

Run: `cargo test --lib tui::app::tests::s_`

Expected: 6 tests FAIL (status assertions don't match — `s` does nothing currently).

- [ ] **Step 4.3: Add `next_status` helper**

Add this free function near the top of `src/tui/app.rs`, immediately after the `use` block:

```rust
fn next_status(current: &str) -> &'static str {
    match current {
        "open" => "in_progress",
        "in_progress" => "done",
        "done" => "open",
        _ => "open",
    }
}
```

- [ ] **Step 4.4: Add `preserve_cursor_on` helper**

Add this method to `impl App`, immediately after `clamp_cursor`:

```rust
    /// Set cursor to the visible position of `id` (if still visible),
    /// else clamp to the new visible length.
    fn preserve_cursor_on(&mut self, id: i64) {
        let visible = self.visible_indices();
        if let Some(pos) = visible
            .iter()
            .position(|i| self.tasks.get(*i).map(|t| t.id) == Some(id))
        {
            self.cursor = pos;
        } else {
            self.clamp_cursor();
        }
    }
```

- [ ] **Step 4.5: Implement `s` in `handle_list`**

In `src/tui/app.rs`, replace the `handle_list` method:

```rust
    fn handle_list(&mut self, key: KeyEvent) {
        let visible_len = self.visible_indices().len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if visible_len > 0 && self.cursor + 1 < visible_len {
                    self.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                self.cursor = 0;
            }
            KeyCode::Char('G') => {
                self.cursor = visible_len.saturating_sub(1);
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.focus = Pane::Detail;
            }
            KeyCode::Char('f') => {
                self.pending_chord = Some('f');
            }
            KeyCode::Char('s') => {
                self.cycle_status();
            }
            _ => {}
        }
    }
```

- [ ] **Step 4.6: Add `cycle_status` to `impl App`**

Add this method to `impl App`, immediately after `handle_detail`:

```rust
    fn cycle_status(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let id = task.id;
        let next = next_status(&task.status);
        if crate::db::set_status(&self.conn, id, next).is_ok() {
            let _ = self.reload();
            self.preserve_cursor_on(id);
        }
    }
```

- [ ] **Step 4.7: Run the new tests**

Run: `cargo test --lib tui::app::tests::s_`

Expected: 6 tests PASS.

- [ ] **Step 4.8: Run the full suite**

Run: `cargo test`

Expected: 85 tests pass (79 + 6 new). All previously-green tests stay green.

- [ ] **Step 4.9: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): s cycles task status open → in_progress → done → open"
```

---

## Task 5: `d` marks the cursor task done

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 5.1: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn d_marks_open_task_done() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_marks_in_progress_task_done() {
        let mut app = mem_app_with(&[("a", "in_progress", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_on_already_done_task_stays_done() {
        let mut app = mem_app_with(&[("a", "done", 2)]);
        app.handle_key(key(KeyCode::Char('d')));
        assert_eq!(app.selected_task().unwrap().status, "done");
    }

    #[test]
    fn d_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('d')));
        assert!(app.selected_task().is_none());
    }
```

- [ ] **Step 5.2: Run tests to confirm they fail**

Run: `cargo test --lib tui::app::tests::d_`

Expected: 3 of 4 fail. (`d_on_already_done_task_stays_done` will pass since the task starts in `done` and `d` is currently a no-op.)

- [ ] **Step 5.3: Implement `d` in `handle_list`**

In `src/tui/app.rs`, modify the `handle_list` match arm block — add `KeyCode::Char('d')` right after the `KeyCode::Char('s')` arm:

```rust
            KeyCode::Char('s') => {
                self.cycle_status();
            }
            KeyCode::Char('d') => {
                self.mark_done();
            }
```

- [ ] **Step 5.4: Add `mark_done` to `impl App`**

Add this method to `impl App`, immediately after `cycle_status`:

```rust
    fn mark_done(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        let id = task.id;
        if crate::db::set_status(&self.conn, id, "done").is_ok() {
            let _ = self.reload();
            self.preserve_cursor_on(id);
        }
    }
```

- [ ] **Step 5.5: Run the new tests**

Run: `cargo test --lib tui::app::tests::d_`

Expected: 4 tests PASS.

- [ ] **Step 5.6: Run the full suite**

Run: `cargo test`

Expected: 89 tests pass (85 + 4 new).

- [ ] **Step 5.7: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): d marks cursor task done"
```

---

## Task 6: `x` opens the Confirm modal

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 6.1: Remove the `#[allow(unused_imports)]` on `PendingAction`**

In `src/tui/app.rs`, change:

```rust
#[allow(unused_imports)] // PendingAction is referenced in Task 5
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

back to:

```rust
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

- [ ] **Step 6.2: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn x_opens_confirm_modal_for_cursor_task() {
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('x')));
        match &app.screen {
            Screen::Confirm(PendingAction::DeleteTask { id: got_id, title }) => {
                assert_eq!(*got_id, id);
                assert_eq!(title, "hello");
            }
            other => panic!("expected Confirm(DeleteTask), got {:?}", other),
        }
    }

    #[test]
    fn x_on_empty_list_is_a_noop() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(app.screen, Screen::Main);
    }
```

- [ ] **Step 6.3: Run tests to confirm they fail**

Run: `cargo test --lib tui::app::tests::x_`

Expected: 1 of 2 fail. (`x_on_empty_list_is_a_noop` will pass.)

- [ ] **Step 6.4: Implement `x` in `handle_list`**

In `src/tui/app.rs`, modify the `handle_list` match arm block — add `KeyCode::Char('x')` right after the `KeyCode::Char('d')` arm:

```rust
            KeyCode::Char('d') => {
                self.mark_done();
            }
            KeyCode::Char('x') => {
                self.open_delete_confirm();
            }
```

- [ ] **Step 6.5: Add `open_delete_confirm` to `impl App`**

Add this method to `impl App`, immediately after `mark_done`:

```rust
    fn open_delete_confirm(&mut self) {
        let Some(task) = self.selected_task() else {
            return;
        };
        self.screen = Screen::Confirm(PendingAction::DeleteTask {
            id: task.id,
            title: task.title.clone(),
        });
    }
```

- [ ] **Step 6.6: Run the new tests**

Run: `cargo test --lib tui::app::tests::x_`

Expected: 2 tests PASS.

- [ ] **Step 6.7: Run the full suite**

Run: `cargo test`

Expected: 91 tests pass (89 + 2 new).

- [ ] **Step 6.8: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): x opens delete-confirm modal for cursor task"
```

---

## Task 7: Confirm modal handler (y/Enter confirms; n/Esc/q cancels)

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 7.1: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn confirm_y_deletes_task_and_returns_to_main() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        let id_to_delete = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert!(app.tasks.iter().all(|t| t.id != id_to_delete));
    }

    #[test]
    fn confirm_enter_deletes_task_and_returns_to_main() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn confirm_n_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_esc_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_q_cancels_and_keeps_task() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('q')));
        assert_eq!(app.screen, Screen::Main);
        assert!(!app.should_quit);
        assert_eq!(app.tasks.len(), 1);
    }

    #[test]
    fn confirm_delete_clamps_cursor_when_last_task_removed() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        // Cursor on second (last) row.
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Char('y')));
        // After delete, only one task remains; cursor must clamp to 0.
        assert_eq!(app.cursor, 0);
        assert_eq!(app.tasks.len(), 1);
    }
```

- [ ] **Step 7.2: Run tests to confirm they fail**

Run: `cargo test --lib tui::app::tests::confirm_`

Expected: all 6 fail (handle_confirm is a stub).

- [ ] **Step 7.3: Implement `handle_confirm`**

In `src/tui/app.rs`, replace the stub `handle_confirm` method:

```rust
    fn handle_confirm(&mut self, key: KeyEvent) {
        let confirmed = matches!(key.code, KeyCode::Char('y') | KeyCode::Enter);
        let cancelled = matches!(
            key.code,
            KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Esc
        );

        if !confirmed && !cancelled {
            return;
        }

        if confirmed {
            // Take the pending action so we can mutate `self` freely below.
            let action = match &self.screen {
                Screen::Confirm(a) => a.clone(),
                _ => return,
            };
            match action {
                PendingAction::DeleteTask { id, .. } => {
                    if crate::db::delete_task(&self.conn, id).is_ok() {
                        let _ = self.reload();
                    }
                }
            }
        }

        self.screen = Screen::Main;
    }
```

- [ ] **Step 7.4: Run the new tests**

Run: `cargo test --lib tui::app::tests::confirm_`

Expected: 6 tests PASS.

- [ ] **Step 7.5: Run the full suite**

Run: `cargo test`

Expected: 97 tests pass (91 + 6 new).

- [ ] **Step 7.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): handle Confirm modal (y/Enter deletes, n/Esc/q cancels)"
```

---

## Task 8: Render the Confirm modal

**Files:**
- Modify: `src/tui/screens/confirm.rs`

- [ ] **Step 8.1: Implement the confirm renderer**

Replace the contents of `src/tui/screens/confirm.rs`:

```rust
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::model::fmt_id;
use crate::tui::screens::PendingAction;

pub fn render(frame: &mut Frame, action: &PendingAction) {
    let (title, prompt) = match action {
        PendingAction::DeleteTask { id, title } => (
            " delete task ".to_string(),
            format!("Delete {} \"{}\"?", fmt_id(*id), title),
        ),
    };

    let area = centered_rect(60, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            prompt,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "This cannot be undone.",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y/Enter",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" confirm  ·  "),
            Span::styled(
                "n/Esc",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel"),
        ]),
    ];

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(9),
            Constraint::Percentage(40),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
```

- [ ] **Step 8.2: Add a smoke render test**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn confirm_modal_renders_with_task_title() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app_with(&[("buy milk", "open", 2)]);
        app.handle_key(key(KeyCode::Char('x')));
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(s.contains("delete task"));
        assert!(s.contains("buy milk"));
        assert!(s.contains("y/Enter"));
        assert!(s.contains("n/Esc"));
    }
```

- [ ] **Step 8.3: Run the new test**

Run: `cargo test --lib tui::app::tests::confirm_modal_renders_with_task_title`

Expected: PASS.

- [ ] **Step 8.4: Run the full suite**

Run: `cargo test`

Expected: 98 tests pass (97 + 1 new). All 11 integration tests still green.

- [ ] **Step 8.5: Verify warning-free build**

Run: `cargo build 2>&1 | grep -i warning || echo "no warnings"`

Expected: `no warnings`.

- [ ] **Step 8.6: Commit**

```bash
git add src/tui/screens/confirm.rs src/tui/app.rs
git commit -m "feat(tui): render delete-confirm modal"
```

---

## Task 9: Manual smoke test

No file changes; just verify the wiring end-to-end. Do not commit anything from this task.

- [ ] **Step 9.1: Build**

Run: `cargo build --quiet`

Expected: clean.

- [ ] **Step 9.2: Confirm `--help` still works (proxy for the binary's clap surface)**

Run: `cargo run --quiet -- tui --help`

Expected: prints clap help for the `tui` subcommand without error. (Cannot drive raw-mode interactively from here — the user will verify the TUI itself.)

- [ ] **Step 9.3: Hand off to the user**

Stop and report:
- 8 commits pushed to `tui-design-spec`
- Test count went from 77 → 98
- Ask the user to run `cargo run -- tui`, navigate with `j`/`k`, press `s` to cycle status on a task, `d` to mark one done, `x` then `y` to delete one, and `x` then `Esc` to cancel a delete. Verify the chord paths (`f r` / `f c`) still work.

---

## Push step (after Task 9)

Once the user has verified visually, push:

```bash
git push origin tui-design-spec
```

Then stop. Do **not** open a GitHub PR — user is keeping the work on `tui-design-spec` until all six PRs land.

---

## Self-Review

**Spec coverage** (against §4.2 list-scope + §3.5 Screen state machine):

- §4.2 `s` cycle status forward → Task 4
- §4.2 `Shift+s` cycle status backward → **deferred** (key collides with `S` = start in PR-6; documented in plan header)
- §4.2 `d` mark done → Task 5
- §4.2 `x` delete → Confirm modal → Task 6
- §4.2 `S` start task, `Ctrl+s` start with tmux → out of scope (PR-6)
- §3.5 `Confirm(PendingAction)` variant on `Screen` → Task 1
- Modal render-on-top discipline (Confirm overlays Main, consumes events first) → Task 3 + Task 8
- Custom-status entry into cycle → Task 4 (step 4.3 in `next_status`, tested in `s_cycles_custom_status_to_open`)

**Placeholder scan:** every step has exact code or exact commands. No `TODO`, no "add appropriate error handling", no "similar to Task N".

**Type consistency:**
- `PendingAction::DeleteTask { id: i64, title: String }` — defined in Task 1; constructed in Task 6 (`open_delete_confirm`); pattern-matched in Task 7 (`handle_confirm`) and Task 8 (`confirm::render`). Field names match across all sites.
- `Screen::Confirm(PendingAction)` — constructed in Task 6; pattern-matched in Task 3 (`render`), Task 3 (`handle_key`), and Task 7 (`handle_confirm`).
- `Scope::Confirm` — added in Task 2; footer query covered by `footer_for_confirm_includes_confirm_and_cancel` in Task 2.
- `next_status` is a free fn (not a method); `cycle_status` / `mark_done` / `open_delete_confirm` / `handle_confirm` are methods on `App`. All four added in Tasks 4–7. No name drift between definition and call sites.
- `preserve_cursor_on(id: i64)` defined in Task 4; called from `cycle_status` (Task 4) and `mark_done` (Task 5). Not called from delete because the task no longer exists; `reload()` already calls `clamp_cursor` so the delete path is correct.

**Atomic-commit discipline:**
- Tasks 1+2+3 are scaffolding that don't independently compile (Task 1 alone breaks the exhaustive match; Task 2 alone has no consumer); they ship as one commit per the PR-3 precedent for paired changes.
- Tasks 4 / 5 / 6 / 7 / 8 each ship a single self-contained feature with its tests; each commit is independently green.

**Test count progression:** 77 → 79 (after Task 3) → 85 (Task 4) → 89 (Task 5) → 91 (Task 6) → 97 (Task 7) → 98 (Task 8). The integration suite (`tests/start.rs`) stays at 11 throughout; the refactor seams have not moved.

**Out of scope (deferred to PR-5 / PR-6):**
- `n` / `e` add and edit forms → PR-5
- `Shift+s` backward cycle → permanently deferred (key collides with `S` = start)
- `S` start task, `Ctrl+s` tmux start → PR-6
- `PendingAction::DiscardEdits` for the edit-form dirty-state confirm → PR-5 (the variant will be added when PR-5 needs it)
- Status-line / transient feedback for DB errors (spec §3.4 mentions a `StatusLine`) → deferred until a real error path needs surfacing; for PR-4 the mutations are unlikely to fail (single-row update/delete by id on the local SQLite file) so silent failure on rare error is acceptable.
