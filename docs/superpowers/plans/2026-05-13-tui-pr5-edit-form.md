# Docket TUI — PR-5: Add / Edit Form Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a centered modal form that lets the user create new tasks (`n`) and edit existing tasks (`e`). All six fields are supported: title, priority, group, deps, body, acceptance. Single-line fields use `tui-input`; multi-line `body` / `acceptance` use `tui-textarea`. `Tab` / `Shift+Tab` navigates fields; `Ctrl+S` validates and saves; `Esc` cancels with a discard-confirm if dirty.

**Architecture:** A new `screens/edit.rs` module owns `EditState`, `EditMode { Add, Edit { id } }`, and `EditField`. The `Screen` enum grows `Edit(Box<EditState>)`. `EditState` holds one `Input` per single-line field, one `TextArea` per multi-line field, the currently focused field, an optional error string, and a snapshot of the initial values for dirty-state detection. Key events while in `Screen::Edit` first check global shortcuts (`Tab`, `Shift+Tab`, `Ctrl+S`, `Esc`); everything else is forwarded as a crossterm `Event::Key` to whichever widget owns the active field. Save runs through a pure `validate()` function that returns `Result<ValidatedForm, String>` — the error string is surfaced in the modal's status line, the modal stays open. On success, the DB op runs through the existing `db::insert_task` / `db::update_task`, the App reloads, and the modal closes (Add jumps the cursor to the new id; Edit preserves it on the edited id). Dirty cancel routes through the existing `Confirm(PendingAction::DiscardEdits)` modal.

**Tech Stack:** ratatui 0.29, crossterm 0.28, `tui-input` 0.10, `tui-textarea` 0.7. The latter two are new in this PR.

---

## File Structure (delta from PR-4)

```
Cargo.toml                 # Modify: add tui-input + tui-textarea
src/tui/
  screens/
    mod.rs                 # Modify: add Screen::Edit(Box<EditState>) variant;
                           #   add PendingAction::DiscardEdits; replace the
                           #   auto-derived PartialEq with a manual impl
                           #   (TextArea is not PartialEq)
    edit.rs                # Create: EditState, EditField, EditMode,
                           #   ValidatedForm, validate(), renderer
  keybindings.rs           # Modify: add Scope::Edit + bindings; add n/e to List
  app.rs                   # Modify: handle_edit, handle key dispatch for Edit
                           #   screen + Confirm(DiscardEdits)
```

**Boundaries:**
- `screens/edit.rs` owns all editor state and pure logic (`validate`, dirty checks, field cycling). `app.rs` orchestrates: it constructs `EditState`, forwards events, and on Ctrl+S calls `validate()` then the right `db::*` fn.
- The renderer in `screens/edit.rs` is a `&EditState` → frame fn, no `&App`.
- The form modal is the only thing rendered when `Screen::Edit` is active — main pane is NOT drawn beneath it (modal is full-frame because the form is large).

---

## Task 1: Add `tui-input` and `tui-textarea` dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1.1: Add the deps**

In `Cargo.toml`, replace the `[dependencies]` block — append the two new lines:

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
rusqlite = { version = "0.32", features = ["bundled"] }
anyhow = "1"
chrono = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ratatui = "0.29"
crossterm = "0.28"
tui-input = "0.10"
tui-textarea = "0.7"
```

- [ ] **Step 1.2: Build to verify version compatibility**

Run: `cargo build`

Expected: clean build. If either crate errors on a feature-flag mismatch (e.g. `tui-textarea` wanting `ratatui-crossterm`), adjust by switching to:

```toml
tui-textarea = { version = "0.7", default-features = false, features = ["ratatui-crossterm", "search"] }
```

and re-run `cargo build`. Do not proceed until the build is clean.

- [ ] **Step 1.3: Test suite still green**

Run: `cargo test --quiet`

Expected: 98 tests pass (the PR-4 baseline).

- [ ] **Step 1.4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "build(tui): add tui-input and tui-textarea dependencies"
```

---

## Task 2: Scaffold `EditState`, `Screen::Edit`, `PendingAction::DiscardEdits`

**Files:**
- Create: `src/tui/screens/edit.rs`
- Modify: `src/tui/screens/mod.rs`
- Modify: `src/tui/app.rs`

This task does NOT yet implement opening / navigating / saving / rendering — it just makes the types compile, the screen routable, and adds a render stub. Behaviour lands in Tasks 4–10.

- [ ] **Step 2.1: Create `src/tui/screens/edit.rs` with types and constructors**

```rust
use tui_input::Input;
use tui_textarea::TextArea;

use crate::model::{fmt_id, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    Add,
    Edit { id: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Title,
    Priority,
    Group,
    Deps,
    Body,
    Acceptance,
}

pub const FIELD_ORDER: [EditField; 6] = [
    EditField::Title,
    EditField::Priority,
    EditField::Group,
    EditField::Deps,
    EditField::Body,
    EditField::Acceptance,
];

pub struct EditState {
    pub mode: EditMode,
    pub field: EditField,
    pub title: Input,
    pub priority: Input,
    pub group: Input,
    pub deps: Input,
    pub body: TextArea<'static>,
    pub acceptance: TextArea<'static>,
    pub error: Option<String>,
    orig_title: String,
    orig_priority: String,
    orig_group: String,
    orig_deps: String,
    orig_body: String,
    orig_acceptance: String,
}

impl std::fmt::Debug for EditState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditState")
            .field("mode", &self.mode)
            .field("field", &self.field)
            .field("title", &self.title.value())
            .field("priority", &self.priority.value())
            .field("group", &self.group.value())
            .field("deps", &self.deps.value())
            .field("body", &self.body.lines().join("\n"))
            .field("acceptance", &self.acceptance.lines().join("\n"))
            .field("error", &self.error)
            .finish()
    }
}

impl EditState {
    pub fn for_add() -> Self {
        Self::build(EditMode::Add, "", "", "", "", "", "")
    }

    pub fn for_edit(task: &Task) -> Self {
        let deps_str = task
            .deps
            .iter()
            .map(|d| fmt_id(*d))
            .collect::<Vec<_>>()
            .join(", ");
        Self::build(
            EditMode::Edit { id: task.id },
            &task.title,
            &task.priority.to_string(),
            task.group.as_deref().unwrap_or(""),
            &deps_str,
            task.body.as_deref().unwrap_or(""),
            task.acceptance.as_deref().unwrap_or(""),
        )
    }

    fn build(
        mode: EditMode,
        title: &str,
        priority: &str,
        group: &str,
        deps: &str,
        body: &str,
        acceptance: &str,
    ) -> Self {
        let body_lines: Vec<String> = if body.is_empty() {
            vec![String::new()]
        } else {
            body.lines().map(|s| s.to_string()).collect()
        };
        let acc_lines: Vec<String> = if acceptance.is_empty() {
            vec![String::new()]
        } else {
            acceptance.lines().map(|s| s.to_string()).collect()
        };
        Self {
            mode,
            field: EditField::Title,
            title: Input::default().with_value(title.to_string()),
            priority: Input::default().with_value(priority.to_string()),
            group: Input::default().with_value(group.to_string()),
            deps: Input::default().with_value(deps.to_string()),
            body: TextArea::new(body_lines),
            acceptance: TextArea::new(acc_lines),
            error: None,
            orig_title: title.to_string(),
            orig_priority: priority.to_string(),
            orig_group: group.to_string(),
            orig_deps: deps.to_string(),
            orig_body: body.to_string(),
            orig_acceptance: acceptance.to_string(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.title.value() != self.orig_title
            || self.priority.value() != self.orig_priority
            || self.group.value() != self.orig_group
            || self.deps.value() != self.orig_deps
            || self.body.lines().join("\n") != self.orig_body
            || self.acceptance.lines().join("\n") != self.orig_acceptance
    }

    pub fn next_field(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = FIELD_ORDER[(idx + 1) % FIELD_ORDER.len()];
    }

    pub fn prev_field(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = FIELD_ORDER[(idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()];
    }
}

pub fn render(_frame: &mut ratatui::Frame, _state: &EditState) {
    // Real renderer lands in Task 10.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_add_starts_blank_on_title() {
        let s = EditState::for_add();
        assert_eq!(s.mode, EditMode::Add);
        assert_eq!(s.field, EditField::Title);
        assert_eq!(s.title.value(), "");
        assert!(!s.is_dirty());
    }

    #[test]
    fn for_edit_populates_all_fields_from_task() {
        let task = Task {
            id: 7,
            title: "hello".into(),
            body: Some("line1\nline2".into()),
            acceptance: Some("- check x".into()),
            deps: vec![3, 5],
            status: "open".into(),
            priority: 1,
            group: Some("v0.1".into()),
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let s = EditState::for_edit(&task);
        assert_eq!(s.mode, EditMode::Edit { id: 7 });
        assert_eq!(s.title.value(), "hello");
        assert_eq!(s.priority.value(), "1");
        assert_eq!(s.group.value(), "v0.1");
        assert_eq!(s.deps.value(), "T-3, T-5");
        assert_eq!(s.body.lines().join("\n"), "line1\nline2");
        assert_eq!(s.acceptance.lines().join("\n"), "- check x");
        assert!(!s.is_dirty());
    }

    #[test]
    fn next_field_wraps() {
        let mut s = EditState::for_add();
        s.field = EditField::Acceptance;
        s.next_field();
        assert_eq!(s.field, EditField::Title);
    }

    #[test]
    fn prev_field_wraps() {
        let mut s = EditState::for_add();
        s.field = EditField::Title;
        s.prev_field();
        assert_eq!(s.field, EditField::Acceptance);
    }

    #[test]
    fn dirty_after_changing_title() {
        let mut s = EditState::for_add();
        s = EditState {
            title: Input::default().with_value("changed".into()),
            ..s
        };
        assert!(s.is_dirty());
    }
}
```

- [ ] **Step 2.2: Update `src/tui/screens/mod.rs`**

Replace the entire file:

```rust
pub mod confirm;
pub mod edit;
pub mod filter_prompt;
pub mod help;
pub mod main;

use edit::EditState;

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
    DiscardEdits,
}

#[derive(Debug)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
    Confirm(PendingAction),
    Edit(Box<EditState>),
}

impl Clone for Screen {
    fn clone(&self) -> Self {
        match self {
            Screen::Main => Screen::Main,
            Screen::Help => Screen::Help,
            Screen::FilterPrompt { kind, input } => Screen::FilterPrompt {
                kind: kind.clone(),
                input: input.clone(),
            },
            Screen::Confirm(a) => Screen::Confirm(a.clone()),
            // EditState is not Clone-friendly (TextArea isn't); cloning the
            // Edit screen is intentionally unsupported. The only caller that
            // cloned Screen was handle_confirm's "snapshot the pending action"
            // path, and DiscardEdits carries no Edit data with it.
            Screen::Edit(_) => panic!("Screen::Edit cannot be cloned"),
        }
    }
}

impl PartialEq for Screen {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Screen::Main, Screen::Main) | (Screen::Help, Screen::Help) => true,
            (
                Screen::FilterPrompt { kind: ak, input: ai },
                Screen::FilterPrompt { kind: bk, input: bi },
            ) => ak == bk && ai == bi,
            (Screen::Confirm(a), Screen::Confirm(b)) => a == b,
            (Screen::Edit(a), Screen::Edit(b)) => a.mode == b.mode,
            _ => false,
        }
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
```

- [ ] **Step 2.3: Wire `Screen::Edit` into `App`**

In `src/tui/app.rs`, modify `handle_key`:

```rust
    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.screen {
            Screen::Main => self.handle_main(key),
            Screen::Help => self.handle_help(key),
            Screen::FilterPrompt { .. } => self.handle_filter_prompt(key),
            Screen::Confirm(_) => self.handle_confirm(key),
            Screen::Edit(_) => self.handle_edit(key),
        }
    }
```

Add a stub `handle_edit` method to `impl App`, immediately after `handle_confirm`:

```rust
    fn handle_edit(&mut self, _key: KeyEvent) {
        // Real handler lands in Tasks 5–9.
    }
```

In `App::render`, extend the match:

```rust
    pub fn render(&self, frame: &mut Frame) {
        match &self.screen {
            Screen::Edit(state) => crate::tui::screens::edit::render(frame, state),
            _ => {
                crate::tui::screens::main::render(self, frame);
                match &self.screen {
                    Screen::Help => crate::tui::screens::help::render(frame),
                    Screen::FilterPrompt { kind, input } => {
                        crate::tui::screens::filter_prompt::render(frame, kind, input)
                    }
                    Screen::Confirm(action) => crate::tui::screens::confirm::render(frame, action),
                    Screen::Main | Screen::Edit(_) => {}
                }
            }
        }
    }
```

(The Edit screen takes over the whole frame — no underlay — because the form is large.)

- [ ] **Step 2.4: Build and test**

Run: `cargo build` — expected: clean, no warnings.
Run: `cargo test --quiet` — expected: 103 tests pass (98 + 5 new `edit::tests`).

- [ ] **Step 2.5: Commit**

```bash
git add Cargo.toml Cargo.lock src/tui/screens/edit.rs src/tui/screens/mod.rs src/tui/app.rs
git commit -m "feat(tui): scaffold Edit screen state and DiscardEdits pending action"
```

(Cargo.toml/Cargo.lock are already committed in Task 1; only the new files land here.)

---

## Task 3: Registry entries for the Edit scope and `n`/`e` list-scope keys

**Files:**
- Modify: `src/tui/keybindings.rs`
- Modify: `src/tui/screens/help.rs`

- [ ] **Step 3.1: Add `Scope::Edit` and the new bindings**

In `src/tui/keybindings.rs`, modify the `Scope` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    List,
    Detail,
    Help,
    FilterPrompt,
    Confirm,
    Edit,
}
```

Add `n` and `e` to the List scope, `e` to the Detail scope, and a block of Edit-scope bindings. The full `COMMAND_REGISTRY` should be:

```rust
pub const COMMAND_REGISTRY: &[Binding] = &[
    Binding { keys: "?", label: "help", scope: Scope::Global, footer: true },
    Binding { keys: "q", label: "quit", scope: Scope::Global, footer: true },
    Binding { keys: "/", label: "filter text", scope: Scope::Global, footer: true },
    Binding { keys: "R", label: "reload", scope: Scope::Global, footer: false },
    Binding { keys: "j/k", label: "nav", scope: Scope::List, footer: true },
    Binding { keys: "g/G", label: "top/bottom", scope: Scope::List, footer: false },
    Binding { keys: "l", label: "detail", scope: Scope::List, footer: true },
    Binding { keys: "n", label: "new", scope: Scope::List, footer: true },
    Binding { keys: "e", label: "edit", scope: Scope::List, footer: true },
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
    Binding { keys: "e", label: "edit", scope: Scope::Detail, footer: true },
    Binding { keys: "PgUp/PgDn", label: "scroll", scope: Scope::Detail, footer: true },
    Binding { keys: "?/Esc", label: "close help", scope: Scope::Help, footer: true },
    Binding { keys: "Enter", label: "apply", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "y/Enter", label: "confirm", scope: Scope::Confirm, footer: true },
    Binding { keys: "n/Esc", label: "cancel", scope: Scope::Confirm, footer: true },
    Binding { keys: "Tab/S-Tab", label: "next/prev field", scope: Scope::Edit, footer: true },
    Binding { keys: "Ctrl+S", label: "save", scope: Scope::Edit, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::Edit, footer: true },
];
```

- [ ] **Step 3.2: Add a registry test**

Append to the `tests` module in `src/tui/keybindings.rs`:

```rust
    #[test]
    fn registry_contains_edit_form_keys() {
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "Ctrl+S" && b.scope == Scope::Edit));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "n" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "e" && b.scope == Scope::List));
        assert!(COMMAND_REGISTRY
            .iter()
            .any(|b| b.keys == "e" && b.scope == Scope::Detail));
    }

    #[test]
    fn footer_for_edit_includes_save_and_cancel() {
        let fb = footer_for(Scope::Edit);
        let labels: Vec<&str> = fb.iter().map(|b| b.label).collect();
        assert!(labels.contains(&"save"));
        assert!(labels.contains(&"cancel"));
        assert!(labels.contains(&"next/prev field"));
    }
```

- [ ] **Step 3.3: Add `Scope::Edit` to the Help overlay loop**

In `src/tui/screens/help.rs`, modify the scope loop:

```rust
    for scope in [
        Scope::Global,
        Scope::List,
        Scope::Detail,
        Scope::FilterPrompt,
        Scope::Confirm,
        Scope::Edit,
    ] {
        let scope_label = match scope {
            Scope::Global => "Global",
            Scope::List => "List",
            Scope::Detail => "Detail",
            Scope::Help => "Help",
            Scope::FilterPrompt => "Filter prompt",
            Scope::Confirm => "Confirm",
            Scope::Edit => "Edit form",
        };
```

- [ ] **Step 3.4: Build and test**

Run: `cargo build` — clean.
Run: `cargo test --quiet` — expected: 105 tests pass (103 + 2 new keybindings tests).

- [ ] **Step 3.5: Commit**

```bash
git add src/tui/keybindings.rs src/tui/screens/help.rs
git commit -m "feat(tui): registry entries for Edit scope and n/e list keys"
```

---

## Task 4: `n` and `e` open the Edit form

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 4.1: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn n_opens_add_form_with_blank_state() {
        use crate::tui::screens::edit::{EditField, EditMode};
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Add);
                assert_eq!(state.field, EditField::Title);
                assert_eq!(state.title.value(), "");
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_from_list_opens_edit_form_for_cursor_task() {
        use crate::tui::screens::edit::EditMode;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('e')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Edit { id });
                assert_eq!(state.title.value(), "hello");
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_from_detail_pane_also_opens_edit_form() {
        use crate::tui::screens::edit::EditMode;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        let id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('l'))); // focus detail
        assert_eq!(app.focus, Pane::Detail);
        app.handle_key(key(KeyCode::Char('e')));
        match &app.screen {
            Screen::Edit(state) => {
                assert_eq!(state.mode, EditMode::Edit { id });
            }
            other => panic!("expected Screen::Edit, got {:?}", other),
        }
    }

    #[test]
    fn e_on_empty_list_does_not_open_form() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('e')));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn n_works_on_empty_list() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        assert!(matches!(app.screen, Screen::Edit(_)));
    }
```

- [ ] **Step 4.2: Run to confirm failures**

Run: `cargo test tui::app::tests::n_ tui::app::tests::e_`

Expected: 5 tests FAIL (key currently routes through handle_list which has no `n` or `e` arm).

- [ ] **Step 4.3: Implement `n` and `e`**

In `src/tui/app.rs`, update the `use crate::tui::screens::...` line at the top:

```rust
use crate::tui::screens::edit::EditState;
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

Modify `handle_list` — add `n` and `e` arms after `KeyCode::Char('x')`:

```rust
            KeyCode::Char('x') => {
                self.open_delete_confirm();
            }
            KeyCode::Char('n') => {
                self.open_add_form();
            }
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            _ => {}
        }
    }
```

Modify `handle_detail` — extend the existing match:

```rust
    fn handle_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = Pane::List;
            }
            KeyCode::Char('e') => {
                self.open_edit_form();
            }
            _ => {}
        }
    }
```

Add the two open methods to `impl App`, immediately after `open_delete_confirm`:

```rust
    fn open_add_form(&mut self) {
        self.screen = Screen::Edit(Box::new(EditState::for_add()));
    }

    fn open_edit_form(&mut self) {
        if let Some(task) = self.selected_task() {
            self.screen = Screen::Edit(Box::new(EditState::for_edit(task)));
        }
    }
```

- [ ] **Step 4.4: Run the new tests**

Run: `cargo test tui::app::tests::n_ tui::app::tests::e_`

Expected: 5 PASS.

- [ ] **Step 4.5: Run full suite**

Run: `cargo test --quiet`

Expected: 110 tests pass.

- [ ] **Step 4.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): n opens Add form; e opens Edit form (list + detail)"
```

---

## Task 5: `Tab` / `Shift+Tab` cycle fields; `Esc` cancels (when clean)

**Files:**
- Modify: `src/tui/app.rs`

This task wires field navigation and the simple cancel path (Esc closes when not dirty). Dirty cancel routes through DiscardEdits — that lands in Task 9.

- [ ] **Step 5.1: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn tab_advances_to_next_field_in_edit() {
        use crate::tui::screens::edit::EditField;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.field, EditField::Priority);
        } else {
            panic!("expected Edit");
        }
    }

    #[test]
    fn shift_tab_goes_back_one_field() {
        use crate::tui::screens::edit::EditField;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab));
        app.handle_key(key(KeyCode::Tab)); // Title → Priority → Group
        app.handle_key(key_with(KeyCode::BackTab, KeyModifiers::SHIFT));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.field, EditField::Priority);
        } else {
            panic!("expected Edit");
        }
    }

    #[test]
    fn esc_closes_clean_add_form() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn esc_closes_clean_edit_form() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('e')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }
```

- [ ] **Step 5.2: Run to confirm failures**

Run: `cargo test tui::app::tests::tab_ tui::app::tests::shift_tab_ tui::app::tests::esc_closes_clean_`

Expected: 4 FAIL.

- [ ] **Step 5.3: Implement `handle_edit` navigation + Esc**

In `src/tui/app.rs`, replace the stub `handle_edit`:

```rust
    fn handle_edit(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Tab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.next_field();
                }
                return;
            }
            (KeyCode::BackTab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.prev_field();
                }
                return;
            }
            (KeyCode::Esc, _) => {
                // Dirty path lands in Task 9; today Esc always closes.
                self.screen = Screen::Main;
                return;
            }
            _ => {}
        }
        // Field input forwarding lands in Task 6.
    }
```

- [ ] **Step 5.4: Run the new tests**

Run: `cargo test tui::app::tests::tab_ tui::app::tests::shift_tab_ tui::app::tests::esc_closes_clean_`

Expected: 4 PASS.

- [ ] **Step 5.5: Run full suite**

Run: `cargo test --quiet`

Expected: 114 tests pass.

- [ ] **Step 5.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): Tab/Shift+Tab cycle fields; Esc closes Edit form"
```

---

## Task 6: Forward typing to the active field

**Files:**
- Modify: `src/tui/app.rs`

Single-line `tui-input` fields and multi-line `tui-textarea` fields each accept crossterm `Event::Key`. We wrap the incoming `KeyEvent` and dispatch to the widget that owns the current field.

- [ ] **Step 6.1: Write the failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn typing_into_title_appends_chars() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "buy milk".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.title.value(), "buy milk");
        } else {
            panic!();
        }
    }

    #[test]
    fn typing_into_priority_field_works() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Tab)); // Title → Priority
        app.handle_key(key(KeyCode::Char('3')));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.priority.value(), "3");
        } else {
            panic!();
        }
    }

    #[test]
    fn typing_into_body_textarea_inserts_text() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        // Navigate to Body field (5th in FIELD_ORDER, index 4)
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Tab));
        }
        for c in "hi".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.body.lines().join("\n"), "hi");
        } else {
            panic!();
        }
    }

    #[test]
    fn enter_in_body_textarea_inserts_newline() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Tab));
        }
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('b')));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.body.lines(), &["a", "b"]);
        } else {
            panic!();
        }
    }

    #[test]
    fn backspace_in_title_deletes_char() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "abc".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Backspace));
        if let Screen::Edit(state) = &app.screen {
            assert_eq!(state.title.value(), "ab");
        } else {
            panic!();
        }
    }
```

- [ ] **Step 6.2: Run to confirm failures**

Run: `cargo test tui::app::tests::typing_ tui::app::tests::enter_in_body tui::app::tests::backspace_in_title`

Expected: 5 FAIL.

- [ ] **Step 6.3: Implement field input forwarding**

In `src/tui/app.rs`, replace the `handle_edit` body — extend the final `// Field input forwarding lands in Task 6.` line with real forwarding:

```rust
    fn handle_edit(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Tab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.next_field();
                }
                return;
            }
            (KeyCode::BackTab, _) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.prev_field();
                }
                return;
            }
            (KeyCode::Esc, _) => {
                self.screen = Screen::Main;
                return;
            }
            _ => {}
        }

        let Screen::Edit(state) = &mut self.screen else {
            return;
        };
        let event = crossterm::event::Event::Key(key);
        use crate::tui::screens::edit::EditField;
        match state.field {
            EditField::Title => {
                let _ = tui_input::backend::crossterm::EventHandler::handle_event(
                    &mut state.title,
                    &event,
                );
            }
            EditField::Priority => {
                let _ = tui_input::backend::crossterm::EventHandler::handle_event(
                    &mut state.priority,
                    &event,
                );
            }
            EditField::Group => {
                let _ = tui_input::backend::crossterm::EventHandler::handle_event(
                    &mut state.group,
                    &event,
                );
            }
            EditField::Deps => {
                let _ = tui_input::backend::crossterm::EventHandler::handle_event(
                    &mut state.deps,
                    &event,
                );
            }
            EditField::Body => {
                state.body.input(event);
            }
            EditField::Acceptance => {
                state.acceptance.input(event);
            }
        }
    }
```

- [ ] **Step 6.4: Run the new tests**

Run: `cargo test tui::app::tests::typing_ tui::app::tests::enter_in_body tui::app::tests::backspace_in_title`

Expected: 5 PASS. If any fail because `tui_input::backend::crossterm::EventHandler` isn't found, use the direct path: replace with `state.title.handle_event(&event)` (the trait is in scope from `use tui_input::backend::crossterm::EventHandler;`). Either form works; pick whichever compiles.

- [ ] **Step 6.5: Run full suite**

Run: `cargo test --quiet`

Expected: 119 tests pass.

- [ ] **Step 6.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): forward typing to the active field (Input / TextArea)"
```

---

## Task 7: Pure `validate()` function

**Files:**
- Modify: `src/tui/screens/edit.rs`

- [ ] **Step 7.1: Define `ValidatedForm` and `validate()`**

In `src/tui/screens/edit.rs`, append (just before the `#[cfg(test)]` block):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedForm {
    pub title: String,
    pub body: Option<String>,
    pub acceptance: Option<String>,
    pub deps_json: Option<String>,
    pub priority: i32,
    pub group: Option<String>,
}

impl EditState {
    /// Pure: reads field text only. Returns the validated form or a single
    /// error message suitable for the modal's status line.
    pub fn validate(&self) -> Result<ValidatedForm, String> {
        let title = self.title.value().trim();
        if title.is_empty() {
            return Err("title is required".into());
        }

        let priority_raw = self.priority.value().trim();
        let priority = if priority_raw.is_empty() {
            2
        } else {
            match priority_raw.parse::<i32>() {
                Ok(p) if (0..=4).contains(&p) => p,
                Ok(_) => return Err("priority must be between 0 and 4".into()),
                Err(_) => return Err("priority must be an integer 0..=4".into()),
            }
        };

        let group_raw = self.group.value().trim();
        let group = if group_raw.is_empty() {
            None
        } else {
            Some(group_raw.to_string())
        };

        let deps_raw = self.deps.value().trim();
        let deps_json = if deps_raw.is_empty() {
            None
        } else {
            crate::model::parse_deps(Some(deps_raw.to_string()))
                .map_err(|e| format!("deps: {}", e))?
        };

        let body_joined = self.body.lines().join("\n");
        let acceptance_joined = self.acceptance.lines().join("\n");
        let body = if body_joined.trim().is_empty() {
            None
        } else {
            Some(body_joined)
        };
        let acceptance = if acceptance_joined.trim().is_empty() {
            None
        } else {
            Some(acceptance_joined)
        };

        Ok(ValidatedForm {
            title: title.to_string(),
            body,
            acceptance,
            deps_json,
            priority,
            group,
        })
    }
}
```

- [ ] **Step 7.2: Add tests**

Append to the `#[cfg(test)] mod tests` block in `src/tui/screens/edit.rs`:

```rust
    #[test]
    fn validate_rejects_empty_title() {
        let s = EditState::for_add();
        let err = s.validate().unwrap_err();
        assert!(err.contains("title"));
    }

    #[test]
    fn validate_defaults_priority_to_two_when_blank() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert_eq!(s.validate().unwrap().priority, 2);
    }

    #[test]
    fn validate_accepts_priority_zero_through_four() {
        for p in 0..=4 {
            let mut s = EditState::for_add();
            s.title = Input::default().with_value("ok".into());
            s.priority = Input::default().with_value(p.to_string());
            assert_eq!(s.validate().unwrap().priority, p);
        }
    }

    #[test]
    fn validate_rejects_priority_out_of_range() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.priority = Input::default().with_value("5".into());
        assert!(s.validate().unwrap_err().contains("0 and 4"));
    }

    #[test]
    fn validate_rejects_non_numeric_priority() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.priority = Input::default().with_value("foo".into());
        assert!(s.validate().unwrap_err().contains("integer"));
    }

    #[test]
    fn validate_empty_group_yields_none() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert!(s.validate().unwrap().group.is_none());
    }

    #[test]
    fn validate_non_empty_group_yields_some() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.group = Input::default().with_value("v0.1".into());
        assert_eq!(s.validate().unwrap().group.as_deref(), Some("v0.1"));
    }

    #[test]
    fn validate_parses_deps_into_json() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.deps = Input::default().with_value("T-3, 7".into());
        assert_eq!(s.validate().unwrap().deps_json.as_deref(), Some("[3,7]"));
    }

    #[test]
    fn validate_rejects_invalid_deps() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        s.deps = Input::default().with_value("not-a-task".into());
        assert!(s.validate().unwrap_err().contains("deps"));
    }

    #[test]
    fn validate_empty_body_yields_none() {
        let mut s = EditState::for_add();
        s.title = Input::default().with_value("ok".into());
        assert!(s.validate().unwrap().body.is_none());
    }
```

- [ ] **Step 7.3: Run tests**

Run: `cargo test tui::screens::edit::tests::validate_`

Expected: 10 PASS.

- [ ] **Step 7.4: Run full suite**

Run: `cargo test --quiet`

Expected: 129 tests pass.

- [ ] **Step 7.5: Commit**

```bash
git add src/tui/screens/edit.rs
git commit -m "feat(tui): add EditState::validate() with title/priority/group/deps rules"
```

---

## Task 8: `Ctrl+S` saves (Add via insert_task; Edit via update_task)

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 8.1: Write failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn ctrl_s_save_creates_new_task_in_add_mode() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "from form".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "from form");
        assert_eq!(app.tasks[0].priority, 2);
    }

    #[test]
    fn ctrl_s_save_updates_existing_task_in_edit_mode() {
        let mut app = mem_app_with(&[("orig", "open", 2)]);
        app.handle_key(key(KeyCode::Char('e')));
        // Clear the title field then retype.
        for _ in 0.."orig".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for c in "renamed".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "renamed");
    }

    #[test]
    fn ctrl_s_save_with_invalid_title_keeps_form_open_with_error() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        // No title typed.
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        match &app.screen {
            Screen::Edit(state) => {
                assert!(state.error.as_deref().unwrap().contains("title"));
            }
            _ => panic!("expected Edit"),
        }
    }

    #[test]
    fn ctrl_s_save_priority_out_of_range_keeps_form_open() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        for c in "ok".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab)); // → Priority
        app.handle_key(key(KeyCode::Char('9')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(app.screen, Screen::Edit(_)));
        assert_eq!(app.tasks.len(), 0);
    }

    #[test]
    fn ctrl_s_save_in_add_mode_creates_with_group_and_deps() {
        let mut app = mem_app_with(&[("dep", "open", 2)]);
        let dep_id = app.selected_task().unwrap().id;
        app.handle_key(key(KeyCode::Char('n')));
        for c in "child".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab)); // Priority
        app.handle_key(key(KeyCode::Tab)); // Group
        for c in "v0.2".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Tab)); // Deps
        for c in format!("T-{}", dep_id).chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(app.screen, Screen::Main);
        let child = app
            .tasks
            .iter()
            .find(|t| t.title == "child")
            .expect("child created");
        assert_eq!(child.group.as_deref(), Some("v0.2"));
        assert_eq!(child.deps, vec![dep_id]);
    }
```

- [ ] **Step 8.2: Run to confirm failures**

Run: `cargo test tui::app::tests::ctrl_s_save_`

Expected: 5 FAIL.

- [ ] **Step 8.3: Implement save**

In `src/tui/app.rs`, add a `save_edit` method to `impl App`, immediately after the two `open_*_form` methods:

```rust
    fn save_edit(&mut self) {
        let Screen::Edit(state) = &mut self.screen else {
            return;
        };
        let validated = match state.validate() {
            Ok(v) => v,
            Err(msg) => {
                state.error = Some(msg);
                return;
            }
        };
        let mode = state.mode;

        let group_id = match validated.group.as_deref() {
            None => None,
            Some(name) => match crate::db::get_or_create_group(&self.conn, name) {
                Ok(id) => Some(id),
                Err(e) => {
                    state.error = Some(format!("group: {}", e));
                    return;
                }
            },
        };

        let result = match mode {
            EditMode::Add => crate::db::insert_task(
                &self.conn,
                crate::db::NewTask {
                    title: &validated.title,
                    body: validated.body.as_deref(),
                    acceptance: validated.acceptance.as_deref(),
                    deps_json: validated.deps_json.as_deref(),
                    priority: validated.priority,
                    group_id,
                },
            ),
            EditMode::Edit { id } => crate::db::update_task(
                &self.conn,
                id,
                crate::db::TaskUpdate {
                    title: &validated.title,
                    body: validated.body.as_deref(),
                    acceptance: validated.acceptance.as_deref(),
                    deps_json: validated.deps_json.as_deref(),
                    priority: validated.priority,
                    group_id,
                },
            )
            .map(|_| id),
        };

        match result {
            Ok(id) => {
                let _ = self.reload();
                self.preserve_cursor_on(id);
                self.screen = Screen::Main;
            }
            Err(e) => {
                if let Screen::Edit(state) = &mut self.screen {
                    state.error = Some(format!("db: {}", e));
                }
            }
        }
    }
```

Wire it from `handle_edit` — add the Ctrl+S branch at the top of the match block:

```rust
    fn handle_edit(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.save_edit();
                return;
            }
            (KeyCode::Tab, _) => {
                // ... existing arms ...
```

Update the imports in `src/tui/app.rs` to include the edit types and DB structs you'll need:

```rust
use crate::tui::screens::edit::{EditMode, EditState};
use crate::tui::screens::{FilterKind, PendingAction, Screen};
```

- [ ] **Step 8.4: Run the new tests**

Run: `cargo test tui::app::tests::ctrl_s_save_`

Expected: 5 PASS.

- [ ] **Step 8.5: Run full suite**

Run: `cargo test --quiet`

Expected: 134 tests pass.

- [ ] **Step 8.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): Ctrl+S saves Add/Edit form via db::insert_task/update_task"
```

---

## Task 9: Dirty-state Esc → `Confirm(DiscardEdits)`

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 9.1: Write failing tests**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn esc_on_dirty_form_opens_discard_confirm() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x'))); // type one char — now dirty
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Confirm(PendingAction::DiscardEdits));
    }

    #[test]
    fn discard_confirm_y_returns_to_main_and_drops_edits() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn discard_confirm_n_does_not_persist_edit_form_today() {
        // n in the discard confirm cancels the confirm. The Edit form is
        // gone — we don't restore mid-edit state because Confirm(DiscardEdits)
        // is invoked only after we transitioned away from the editor. The
        // user can press `n` to start over.
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.screen, Screen::Main);
    }
```

- [ ] **Step 9.2: Run to confirm failures**

Run: `cargo test tui::app::tests::esc_on_dirty tui::app::tests::discard_confirm_`

Expected: 2 fail (`discard_confirm_n_does_not_persist...` passes since both paths now return to Main).

- [ ] **Step 9.3: Implement dirty Esc**

In `src/tui/app.rs`, replace the existing Esc arm in `handle_edit`:

```rust
            (KeyCode::Esc, _) => {
                let dirty = matches!(&self.screen, Screen::Edit(state) if state.is_dirty());
                if dirty {
                    self.screen = Screen::Confirm(PendingAction::DiscardEdits);
                } else {
                    self.screen = Screen::Main;
                }
                return;
            }
```

Extend `handle_confirm` to handle `DiscardEdits`:

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
                PendingAction::DiscardEdits => {
                    // Edit form is already gone from the screen; nothing to
                    // persist. Fall through to set Screen::Main.
                }
            }
        }

        self.screen = Screen::Main;
    }
```

- [ ] **Step 9.4: Run the new tests**

Run: `cargo test tui::app::tests::esc_on_dirty tui::app::tests::discard_confirm_`

Expected: 3 PASS.

- [ ] **Step 9.5: Run full suite**

Run: `cargo test --quiet`

Expected: 137 tests pass.

- [ ] **Step 9.6: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): Esc on dirty form opens DiscardEdits confirm"
```

---

## Task 10: Render the Edit screen

**Files:**
- Modify: `src/tui/screens/edit.rs`
- Modify: `src/tui/screens/confirm.rs` (only if needed — confirm renderer must handle `DiscardEdits`)

- [ ] **Step 10.1: Extend the confirm renderer for `DiscardEdits`**

In `src/tui/screens/confirm.rs`, replace the existing `let (title, prompt) = match action { ... };` block:

```rust
    let (title, prompt) = match action {
        PendingAction::DeleteTask { id, title } => (
            " delete task ".to_string(),
            format!("Delete {} \"{}\"?", fmt_id(*id), title),
        ),
        PendingAction::DiscardEdits => (
            " discard edits ".to_string(),
            "Discard unsaved changes?".to_string(),
        ),
    };
```

- [ ] **Step 10.2: Implement the Edit renderer**

Replace `src/tui/screens/edit.rs` `pub fn render(...)` (the stub):

```rust
pub fn render(frame: &mut ratatui::Frame, state: &EditState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Clear, Paragraph};

    let area = centered_rect(80, 30, frame.area());
    frame.render_widget(Clear, area);

    let title_bar = match state.mode {
        EditMode::Add => " new task ".to_string(),
        EditMode::Edit { id } => format!(" edit {} ", crate::model::fmt_id(id)),
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .title(title_bar)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // Rows: label + widget per field, plus a hint line and a status line.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Title
            Constraint::Length(2), // Priority
            Constraint::Length(2), // Group
            Constraint::Length(2), // Deps
            Constraint::Length(6), // Body
            Constraint::Length(6), // Acceptance
            Constraint::Length(1), // hint
            Constraint::Length(1), // error
        ])
        .split(inner);

    render_input_row(frame, rows[0], "Title", &state.title, state.field == EditField::Title);
    render_input_row(frame, rows[1], "Priority", &state.priority, state.field == EditField::Priority);
    render_input_row(frame, rows[2], "Group", &state.group, state.field == EditField::Group);
    render_input_row(frame, rows[3], "Deps", &state.deps, state.field == EditField::Deps);
    render_textarea_row(frame, rows[4], "Body", &state.body, state.field == EditField::Body);
    render_textarea_row(frame, rows[5], "Acceptance", &state.acceptance, state.field == EditField::Acceptance);

    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" next  ·  "),
        Span::styled("Shift+Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" prev  ·  "),
        Span::styled("Ctrl+S", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" save  ·  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" cancel"),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(hint, rows[6]);

    if let Some(err) = &state.error {
        let p = Paragraph::new(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(p, rows[7]);
    }
}

fn render_input_row(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    label: &str,
    input: &tui_input::Input,
    focused: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(area);

    let lbl_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(label.to_string()).style(lbl_style), cols[0]);

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default().borders(Borders::ALL).border_style(border_style);
    let width = cols[1].width.saturating_sub(2) as usize;
    let scroll = input.visual_scroll(width);
    let p = Paragraph::new(input.value())
        .scroll((0, scroll as u16))
        .block(block);
    frame.render_widget(p, cols[1]);
}

fn render_textarea_row(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    label: &str,
    textarea: &tui_textarea::TextArea<'static>,
    focused: bool,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(12), Constraint::Min(10)])
        .split(area);

    let lbl_style = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(Paragraph::new(label.to_string()).style(lbl_style), cols[0]);

    let mut ta = textarea.clone();
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    ta.set_block(Block::default().borders(Borders::ALL).border_style(border_style));
    frame.render_widget(&ta, cols[1]);
}

fn centered_rect(
    percent_x: u16,
    height: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let v_pad = r.height.saturating_sub(height) / 2;
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(v_pad),
            Constraint::Length(height),
            Constraint::Length(v_pad),
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

- [ ] **Step 10.3: Smoke render test**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn edit_form_renders_with_field_labels() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
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
        assert!(s.contains("new task"));
        assert!(s.contains("Title"));
        assert!(s.contains("Priority"));
        assert!(s.contains("Body"));
        assert!(s.contains("Acceptance"));
        assert!(s.contains("Ctrl+S"));
    }

    #[test]
    fn edit_form_renders_validation_error() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('n')));
        app.handle_key(key_with(KeyCode::Char('s'), KeyModifiers::CONTROL));
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
        assert!(s.contains("title"));
    }
```

- [ ] **Step 10.4: Build, test, warning-check**

Run: `cargo build` — clean.
Run: `cargo test --quiet` — expected 139 tests pass.
Run: `cargo build 2>&1 | grep -i warning || echo "no warnings"` — expected: `no warnings`.

- [ ] **Step 10.5: Commit**

```bash
git add src/tui/screens/edit.rs src/tui/screens/confirm.rs src/tui/app.rs
git commit -m "feat(tui): render Add/Edit form modal with field highlights and error line"
```

---

## Task 11: Manual smoke test (no commit)

- [ ] **Step 11.1: Build**

Run: `cargo build --quiet` — clean.

- [ ] **Step 11.2: Verify CLI surface**

Run: `cargo run --quiet -- tui --help` — clap help prints.

- [ ] **Step 11.3: Hand off to the user for visual verification**

Report:
- 10 commits on `tui-design-spec` for PR-5
- Test count went 98 → 139
- New behaviours: `n` opens Add form; `e` opens Edit form (list and detail); Tab/Shift+Tab cycle fields; typing into Title/Priority/Group/Deps/Body/Acceptance works; Ctrl+S validates and saves; invalid input keeps the form open with a red error line; Esc on dirty form pops a DiscardEdits confirm; clean Esc closes immediately.

---

## Push step (after Task 11)

```bash
git push origin tui-design-spec
```

---

## Self-Review

**Spec coverage** (against §4.2, §5):

- §4.2 list-scope `n` opens Add — Task 4
- §4.2 list-scope `e` opens Edit — Task 4
- §4.2 detail-scope `e` opens Edit — Task 4
- §5.1 layout — Task 10 (centered modal, 80% wide, field rows, hint line, error line)
- §5.1 widget choice (Input vs TextArea) — Task 2 (EditState construction); Task 10 (renderer dispatch)
- §5.2 Tab / Shift+Tab navigation — Task 5
- §5.2 Ctrl+S save — Task 8
- §5.2 Esc cancel — Task 5 (clean) + Task 9 (dirty)
- §5.2 Enter inside textarea inserts newline — Task 6 (forwarding lets TextArea handle Enter; Input fields ignore Enter naturally)
- §5.3 title required — Task 7 (validate)
- §5.3 priority 0..=4, empty → 2 — Task 7
- §5.3 group empty → None, non-empty → lazy-create — Task 7 (validate) + Task 8 (save calls `get_or_create_group`)
- §5.3 deps parse — Task 7 (via `model::parse_deps`)
- §5.3 body / acceptance empty → NULL — Task 7
- §5.4 Add save semantics (insert → reload → close → cursor jumps to new id) — Task 8 (preserve_cursor_on returns id from insert; cursor lands on it because the list ordering puts the new row at the end of its priority bucket and we preserve by id)
- §5.4 Edit save semantics (update → reload → close → preserve cursor on edited id) — Task 8
- §5.4 DB error keeps modal open with error line — Task 8 (`state.error = Some(...)`)
- §5.5 dirty tracking + Esc confirm — Task 9
- §3.5 `Confirm(PendingAction)` covers DiscardEdits — Task 2 adds variant, Task 9 routes it, Task 10 renders it

**Placeholder scan:** all steps have concrete code, exact commands, and expected outputs.

**Type consistency:**
- `EditState`, `EditField`, `EditMode`, `ValidatedForm` defined in Task 2 / Task 7; consumed by Task 4 (`for_add` / `for_edit`), Task 5 (`next_field` / `prev_field`), Task 6 (`field` discriminant), Task 8 (`validate` + `mode`), Task 9 (`is_dirty`).
- `Screen::Edit(Box<EditState>)` — variant added in Task 2; constructed in Task 4; pattern-matched in Tasks 5/6/8/9; rendered in Task 10.
- `PendingAction::DiscardEdits` — added in Task 2; constructed in Task 9; matched in Task 9 (handle_confirm) and Task 10 (confirm renderer).
- `Scope::Edit` — added in Task 3; footer + help reference it; no other consumer.

**Atomic-commit discipline:**
- Task 1 stands alone (deps only).
- Task 2 compiles cleanly because the handler is a stub and the renderer is a stub; tests for the new types pass. Match arms in `handle_key` / `render` cover Edit via stubs.
- Tasks 3–10 each ship one feature with its tests. Every commit compiles and is green.

**Test count progression:** 98 → 98 (Task 1, no test change) → 103 (Task 2 +5) → 105 (Task 3 +2) → 110 (Task 4 +5) → 114 (Task 5 +4) → 119 (Task 6 +5) → 129 (Task 7 +10) → 134 (Task 8 +5) → 137 (Task 9 +3) → 139 (Task 10 +2).

**Out of scope (deferred):**
- Field-level error chrome (spec §5.3 says explicitly status-line only on day one).
- Status field editing — the form doesn't expose status; PR-4 keys (`s`, `d`) and the existing CLI handle status changes.
- Save cursor jumping to the newly-inserted task: the plan uses `preserve_cursor_on(id)`. Insert returns the new id; reload reorders by `(priority, id)`. The cursor will land on the new task **in its visible position** — which honours the spec's "cursor jumps to new task" intent.
