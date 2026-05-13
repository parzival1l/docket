# Docket TUI — PR-3: Main Screen (Read-Only) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a read-only two-pane main screen: filtered task list on the left, task detail on the right, with all filters (status / group / priority cap / ready / blocked / free-text), a help overlay, a contextual footer, and top-bar filter chips. No mutations yet — those land in PR-4.

**Architecture:** `App` owns a snapshot of `Vec<Task>` and a `Filters` value. Rendering is pure: `App::render` reads state and emits a frame; key handlers mutate state and never render. Filters are applied at render time by `App::filtered_indices()` which returns a `Vec<usize>` of indices into `self.tasks` matching every active filter. Screens (`Main` / `Help` / `FilterPrompt`) form a small state machine — modal screens render over `Main` and consume key events before the list/detail do. Chords use a single `pending_chord: Option<char>` field; pressing `f` then `s` means "open the status filter prompt".

**Tech Stack:** ratatui 0.29, crossterm 0.28. No new deps. (`tui-input` / `tui-textarea` still deferred to PR-5.)

---

## File Structure (delta from PR-2)

```
src/tui/
  mod.rs           # Modify: pass App's DB connection into run_tui (load tasks once)
  app.rs           # Modify: grow App to own Connection, tasks, filters, cursor, focus, screen, chord
  filters.rs       # Create: Filters struct + apply logic + tests
  keybindings.rs   # Create: Scope enum, Command struct, COMMAND_REGISTRY, footer helpers
  screens/
    mod.rs         # Create: Screen enum
    main.rs        # Create: render two-pane main screen + top bar + footer
    help.rs        # Create: render help overlay
    filter_prompt.rs # Create: render single-line filter prompt + handle keys
  widgets/
    mod.rs         # Create
    task_list.rs   # Create: render filtered list with cursor and status colors
    task_detail.rs # Create: render detail of currently-selected task
    footer.rs      # Create: render contextual footer from COMMAND_REGISTRY
    top_bar.rs     # Create: render filter chips
```

**Boundaries:**
- `filters.rs` is pure data → vec-of-indices logic, fully unit-testable without ratatui.
- `keybindings.rs` is a static registry (no app state) — tests assert the registry contents.
- `screens/*.rs` are render-only modules; their pub fn signatures take `&App` and `&mut Frame`.
- `widgets/*.rs` are small render helpers that screens compose.
- Key handling stays inside `App::handle_key` — screens don't have their own handlers (with one exception: `FilterPrompt` accumulates text input, so its handler lives on the screen state struct).

---

## Task 1: Load tasks into App on startup

**Files:**
- Modify: `src/tui/mod.rs`, `src/tui/app.rs`

- [ ] **Step 1.1: Update `App` to own DB state**

Replace the `App` struct at the top of `src/tui/app.rs`:

```rust
use crate::db::{load_all_tasks, open_db};
use crate::model::Task;
use anyhow::Result;
use rusqlite::Connection;

pub struct App {
    pub conn: Connection,
    pub tasks: Vec<Task>,
    pub cursor: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let conn = open_db()?;
        let tasks = load_all_tasks(&conn)?;
        Ok(Self {
            conn,
            tasks,
            cursor: 0,
            should_quit: false,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        self.tasks = load_all_tasks(&self.conn)?;
        if self.cursor >= self.tasks.len() && !self.tasks.is_empty() {
            self.cursor = self.tasks.len() - 1;
        }
        Ok(())
    }
}
```

Remove `#[derive(Default)]` from `App` — it can no longer be Default because `Connection` isn't.

- [ ] **Step 1.2: Update `run_tui` in `src/tui/mod.rs`**

Change the body of `run_tui` to construct App via `App::new()`:

```rust
pub fn run_tui() -> Result<()> {
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
    Ok(())
}
```

(The `App::new()` call moves before `TerminalGuard::new()` so that a DB error prints normally without first entering raw mode.)

- [ ] **Step 1.3: Update existing tests in `src/tui/app.rs`**

The `App::default()` tests no longer compile. Add a test helper at the top of the `tests` module that builds an in-memory App, and update the 5 existing tests to use it:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn mem_app() -> App {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_schema(&conn).unwrap();
        let tasks = crate::db::load_all_tasks(&conn).unwrap();
        App {
            conn,
            tasks,
            cursor: 0,
            should_quit: false,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn q_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('c')));
        assert!(!app.should_quit);
    }

    #[test]
    fn unhandled_keys_are_ignored() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(!app.should_quit);
    }

    #[test]
    fn render_does_not_panic_on_test_backend() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = mem_app();
        terminal.draw(|f| app.render(f)).unwrap();
    }
}
```

- [ ] **Step 1.4: Compile and run**

Run: `cargo test tui::app`
Expected: 5 tests pass.

- [ ] **Step 1.5: Commit**

```bash
git add src/tui/app.rs src/tui/mod.rs
git commit -m "feat(tui): load tasks into App at startup"
```

---

## Task 2: Filters module

**Files:**
- Create: `src/tui/filters.rs`
- Modify: `src/tui/mod.rs` (declare module)

- [ ] **Step 2.1: Create `src/tui/filters.rs`**

```rust
use std::collections::HashMap;

use crate::model::Task;

#[derive(Default, Clone)]
pub struct Filters {
    pub status: Option<String>,
    pub group: Option<String>,
    pub priority_cap: Option<i32>,
    pub ready_only: bool,
    pub blocked_only: bool,
    pub text: Option<String>,
}

impl Filters {
    pub fn is_default(&self) -> bool {
        self.status.is_none()
            && self.group.is_none()
            && self.priority_cap.is_none()
            && !self.ready_only
            && !self.blocked_only
            && self.text.is_none()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Returns indices into `tasks` (preserving order) for tasks matching every
/// active filter. `ready` and `blocked` are mutually exclusive in practice —
/// if both are set, no task can satisfy both, so the result is empty.
pub fn filtered_indices(tasks: &[Task], f: &Filters) -> Vec<usize> {
    let by_id: HashMap<i64, &Task> = tasks.iter().map(|t| (t.id, t)).collect();
    let text_needle = f.text.as_deref().map(str::to_lowercase);

    tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| f.status.as_deref().is_none_or(|s| t.status == s))
        .filter(|(_, t)| {
            f.group
                .as_deref()
                .is_none_or(|g| t.group.as_deref() == Some(g))
        })
        .filter(|(_, t)| f.priority_cap.is_none_or(|cap| t.priority <= cap))
        .filter(|(_, t)| {
            if !f.ready_only {
                return true;
            }
            t.status == "open"
                && t.deps
                    .iter()
                    .all(|d| by_id.get(d).is_none_or(|x| x.status == "done"))
        })
        .filter(|(_, t)| {
            if !f.blocked_only {
                return true;
            }
            t.status == "open"
                && t.deps
                    .iter()
                    .any(|d| by_id.get(d).is_some_and(|x| x.status != "done"))
        })
        .filter(|(_, t)| {
            let Some(ref needle) = text_needle else {
                return true;
            };
            let title_lc = t.title.to_lowercase();
            let body_lc = t.body.as_deref().unwrap_or("").to_lowercase();
            title_lc.contains(needle) || body_lc.contains(needle)
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: i64, title: &str, status: &str, priority: i32, deps: Vec<i64>) -> Task {
        Task {
            id,
            title: title.into(),
            body: None,
            acceptance: None,
            deps,
            status: status.into(),
            priority,
            group: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    #[test]
    fn default_passes_everything() {
        let tasks = vec![task(1, "a", "open", 2, vec![]), task(2, "b", "done", 0, vec![])];
        let got = filtered_indices(&tasks, &Filters::default());
        assert_eq!(got, vec![0, 1]);
    }

    #[test]
    fn status_filter_matches_exact() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "in_progress", 2, vec![]),
            task(3, "c", "done", 2, vec![]),
        ];
        let f = Filters {
            status: Some("open".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0]);
    }

    #[test]
    fn priority_cap_keeps_le() {
        let tasks = vec![
            task(1, "a", "open", 0, vec![]),
            task(2, "b", "open", 2, vec![]),
            task(3, "c", "open", 4, vec![]),
        ];
        let f = Filters {
            priority_cap: Some(2),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0, 1]);
    }

    #[test]
    fn ready_only_excludes_open_with_unmet_deps() {
        let tasks = vec![
            task(1, "a", "done", 2, vec![]),
            task(2, "b", "open", 2, vec![1]), // deps on done -> ready
            task(3, "c", "open", 2, vec![1, 4]), // T-4 missing -> ready (missing treated as ok)
            task(4, "d", "open", 2, vec![5]), // T-5 doesn't exist; missing dep treated as satisfied
        ];
        // Add an open dep to verify blocking
        let mut tasks = tasks;
        tasks.push(task(5, "e", "open", 2, vec![]));
        tasks.push(task(6, "f", "open", 2, vec![5])); // blocked by open T-5
        let f = Filters {
            ready_only: true,
            ..Default::default()
        };
        let got = filtered_indices(&tasks, &f);
        // open tasks with all deps satisfied: T-2 (idx 1), T-3 (idx 2), T-4 (idx 3), T-5 (idx 4)
        // T-6 (idx 5) is blocked by open T-5
        assert_eq!(got, vec![1, 2, 3, 4]);
    }

    #[test]
    fn blocked_only_keeps_open_with_unmet_deps() {
        let tasks = vec![
            task(1, "a", "open", 2, vec![]),
            task(2, "b", "open", 2, vec![1]), // open dep -> blocked
            task(3, "c", "done", 2, vec![]),
        ];
        let f = Filters {
            blocked_only: true,
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![1]);
    }

    #[test]
    fn text_filter_matches_title_case_insensitive() {
        let tasks = vec![
            task(1, "Add tmux delivery", "open", 2, vec![]),
            task(2, "Schema migration", "open", 2, vec![]),
        ];
        let f = Filters {
            text: Some("TMUX".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&tasks, &f), vec![0]);
    }

    #[test]
    fn text_filter_matches_body() {
        let mut t = task(1, "title", "open", 2, vec![]);
        t.body = Some("Lorem ipsum dolor".into());
        let f = Filters {
            text: Some("ipsum".into()),
            ..Default::default()
        };
        assert_eq!(filtered_indices(&[t], &f), vec![0]);
    }

    #[test]
    fn is_default_initially() {
        assert!(Filters::default().is_default());
    }

    #[test]
    fn is_default_false_after_setting_status() {
        let mut f = Filters::default();
        f.status = Some("open".into());
        assert!(!f.is_default());
    }

    #[test]
    fn clear_resets_all() {
        let mut f = Filters {
            status: Some("open".into()),
            ready_only: true,
            text: Some("x".into()),
            ..Default::default()
        };
        f.clear();
        assert!(f.is_default());
    }
}
```

- [ ] **Step 2.2: Declare the module**

In `src/tui/mod.rs`, add near `pub mod app;`:

```rust
pub mod filters;
```

- [ ] **Step 2.3: Run tests**

Run: `cargo test tui::filters`
Expected: 9 tests pass.

- [ ] **Step 2.4: Commit**

```bash
git add src/tui/filters.rs src/tui/mod.rs
git commit -m "feat(tui): add filters module with status/group/priority/ready/blocked/text"
```

---

## Task 3: Keybindings registry

**Files:**
- Create: `src/tui/keybindings.rs`
- Modify: `src/tui/mod.rs`

- [ ] **Step 3.1: Create `src/tui/keybindings.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    List,
    Detail,
    Help,
    FilterPrompt,
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub keys: &'static str,
    pub label: &'static str,
    pub scope: Scope,
    pub footer: bool,
}

pub const COMMAND_REGISTRY: &[Binding] = &[
    // Global
    Binding { keys: "?", label: "help", scope: Scope::Global, footer: true },
    Binding { keys: "q", label: "quit", scope: Scope::Global, footer: true },
    Binding { keys: "/", label: "filter text", scope: Scope::Global, footer: true },
    Binding { keys: "R", label: "reload", scope: Scope::Global, footer: false },
    // List
    Binding { keys: "j/k", label: "nav", scope: Scope::List, footer: true },
    Binding { keys: "g/G", label: "top/bottom", scope: Scope::List, footer: false },
    Binding { keys: "l", label: "detail", scope: Scope::List, footer: true },
    Binding { keys: "f s", label: "filter status", scope: Scope::List, footer: false },
    Binding { keys: "f g", label: "filter group", scope: Scope::List, footer: false },
    Binding { keys: "f p", label: "filter priority", scope: Scope::List, footer: false },
    Binding { keys: "f r", label: "ready", scope: Scope::List, footer: true },
    Binding { keys: "f b", label: "blocked", scope: Scope::List, footer: false },
    Binding { keys: "f c", label: "clear filters", scope: Scope::List, footer: true },
    // Detail
    Binding { keys: "h", label: "list", scope: Scope::Detail, footer: true },
    Binding { keys: "PgUp/PgDn", label: "scroll", scope: Scope::Detail, footer: true },
    // Help
    Binding { keys: "?/Esc", label: "close help", scope: Scope::Help, footer: true },
    // Filter prompt
    Binding { keys: "Enter", label: "apply", scope: Scope::FilterPrompt, footer: true },
    Binding { keys: "Esc", label: "cancel", scope: Scope::FilterPrompt, footer: true },
];

pub fn footer_for(scope: Scope) -> Vec<Binding> {
    COMMAND_REGISTRY
        .iter()
        .filter(|b| b.footer && (b.scope == Scope::Global || b.scope == scope))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_quit_and_help() {
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "q"));
        assert!(COMMAND_REGISTRY.iter().any(|b| b.keys == "?"));
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
}
```

- [ ] **Step 3.2: Declare the module**

Add to `src/tui/mod.rs`:

```rust
pub mod keybindings;
```

- [ ] **Step 3.3: Run tests**

Run: `cargo test tui::keybindings`
Expected: 4 tests pass.

- [ ] **Step 3.4: Commit**

```bash
git add src/tui/keybindings.rs src/tui/mod.rs
git commit -m "feat(tui): add command registry and scope-based footer helpers"
```

---

## Task 4: Screen state machine

**Files:**
- Create: `src/tui/screens/mod.rs`
- Modify: `src/tui/mod.rs`

- [ ] **Step 4.1: Create `src/tui/screens/mod.rs`**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterKind {
    Status,
    Group,
    Priority,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Main,
    Help,
    FilterPrompt {
        kind: FilterKind,
        input: String,
    },
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
```

- [ ] **Step 4.2: Declare and wire**

Add to `src/tui/mod.rs`:

```rust
pub mod screens;
```

- [ ] **Step 4.3: Compile**

Run: `cargo build`
Expected: clean build (no tests yet — Screen has no methods to test).

- [ ] **Step 4.4: Commit**

```bash
git add src/tui/screens/mod.rs src/tui/mod.rs
git commit -m "feat(tui): add Screen state machine"
```

---

## Task 5: Grow App with cursor/focus/screen/chord

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 5.1: Update App fields**

Replace the `App` struct and `impl App` in `src/tui/app.rs` (keep the tests module — we'll update it in step 5.4):

```rust
use crate::db::{load_all_tasks, open_db};
use crate::model::Task;
use crate::tui::filters::{filtered_indices, Filters};
use crate::tui::screens::{FilterKind, Screen};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    List,
    Detail,
}

pub struct App {
    pub conn: Connection,
    pub tasks: Vec<Task>,
    pub filters: Filters,
    pub cursor: usize,
    pub focus: Pane,
    pub screen: Screen,
    pub pending_chord: Option<char>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let conn = open_db()?;
        let tasks = load_all_tasks(&conn)?;
        Ok(Self {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
        })
    }

    pub fn reload(&mut self) -> Result<()> {
        self.tasks = load_all_tasks(&self.conn)?;
        let visible = filtered_indices(&self.tasks, &self.filters);
        if self.cursor >= visible.len() && !visible.is_empty() {
            self.cursor = visible.len() - 1;
        }
        Ok(())
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        filtered_indices(&self.tasks, &self.filters)
    }

    pub fn selected_task(&self) -> Option<&Task> {
        let visible = self.visible_indices();
        visible.get(self.cursor).and_then(|i| self.tasks.get(*i))
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.screen {
            Screen::Main => self.handle_main(key),
            Screen::Help => self.handle_help(key),
            Screen::FilterPrompt { .. } => self.handle_filter_prompt(key),
        }
    }

    fn handle_main(&mut self, key: KeyEvent) {
        // Chord follow-up (only `f <letter>` is supported)
        if let Some('f') = self.pending_chord {
            self.pending_chord = None;
            match key.code {
                KeyCode::Char('s') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Status,
                        input: self.filters.status.clone().unwrap_or_default(),
                    };
                }
                KeyCode::Char('g') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Group,
                        input: self.filters.group.clone().unwrap_or_default(),
                    };
                }
                KeyCode::Char('p') => {
                    self.screen = Screen::FilterPrompt {
                        kind: FilterKind::Priority,
                        input: self
                            .filters
                            .priority_cap
                            .map(|p| p.to_string())
                            .unwrap_or_default(),
                    };
                }
                KeyCode::Char('r') => {
                    self.filters.ready_only = !self.filters.ready_only;
                    if self.filters.ready_only {
                        self.filters.blocked_only = false;
                    }
                    self.clamp_cursor();
                }
                KeyCode::Char('b') => {
                    self.filters.blocked_only = !self.filters.blocked_only;
                    if self.filters.blocked_only {
                        self.filters.ready_only = false;
                    }
                    self.clamp_cursor();
                }
                KeyCode::Char('c') => {
                    self.filters.clear();
                    self.clamp_cursor();
                }
                _ => {}
            }
            return;
        }

        // Global
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => {
                self.should_quit = true;
                return;
            }
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                return;
            }
            (KeyCode::Char('?'), _) => {
                self.screen = Screen::Help;
                return;
            }
            (KeyCode::Char('/'), _) => {
                self.screen = Screen::FilterPrompt {
                    kind: FilterKind::Text,
                    input: self.filters.text.clone().unwrap_or_default(),
                };
                return;
            }
            (KeyCode::Char('R'), _) => {
                let _ = self.reload();
                return;
            }
            _ => {}
        }

        // Pane-specific
        match self.focus {
            Pane::List => self.handle_list(key),
            Pane::Detail => self.handle_detail(key),
        }
    }

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
            _ => {}
        }
    }

    fn handle_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.focus = Pane::List;
            }
            // Scroll keys are handled by the rendered widget; we don't track scroll state yet.
            _ => {}
        }
    }

    fn handle_help(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q')) {
            self.screen = Screen::Main;
        }
    }

    fn handle_filter_prompt(&mut self, key: KeyEvent) {
        let Screen::FilterPrompt { kind, input } = &mut self.screen else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.pending_chord = None;
            }
            KeyCode::Enter => {
                let value = input.trim().to_string();
                let empty = value.is_empty();
                match kind {
                    FilterKind::Status => {
                        self.filters.status = if empty { None } else { Some(value) };
                    }
                    FilterKind::Group => {
                        self.filters.group = if empty { None } else { Some(value) };
                    }
                    FilterKind::Priority => {
                        self.filters.priority_cap = if empty {
                            None
                        } else {
                            value.parse::<i32>().ok()
                        };
                    }
                    FilterKind::Text => {
                        self.filters.text = if empty { None } else { Some(value) };
                    }
                }
                self.screen = Screen::Main;
                self.clamp_cursor();
            }
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) => {
                input.push(c);
            }
            _ => {}
        }
    }

    fn clamp_cursor(&mut self) {
        let visible_len = self.visible_indices().len();
        if visible_len == 0 {
            self.cursor = 0;
        } else if self.cursor >= visible_len {
            self.cursor = visible_len - 1;
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        crate::tui::screens::main::render(self, frame);
        match &self.screen {
            Screen::Help => crate::tui::screens::help::render(frame),
            Screen::FilterPrompt { kind, input } => {
                crate::tui::screens::filter_prompt::render(frame, kind, input)
            }
            Screen::Main => {}
        }
    }
}
```

- [ ] **Step 5.2: Update existing tests to use new App constructor and add new tests**

Replace the `tests` module at the bottom of `src/tui/app.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{init_schema, insert_task, NewTask};
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn mem_app() -> App {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        let tasks = load_all_tasks(&conn).unwrap();
        App {
            conn,
            tasks,
            filters: Filters::default(),
            cursor: 0,
            focus: Pane::List,
            screen: Screen::Main,
            pending_chord: None,
            should_quit: false,
        }
    }

    fn mem_app_with(seed: &[(&str, &str, i32)]) -> App {
        let mut app = mem_app();
        for (title, status, priority) in seed {
            let id = insert_task(
                &app.conn,
                NewTask {
                    title,
                    body: None,
                    acceptance: None,
                    deps_json: None,
                    priority: *priority,
                    group_id: None,
                },
            )
            .unwrap();
            if *status != "open" {
                crate::db::set_status(&app.conn, id, status).unwrap();
            }
        }
        app.reload().unwrap();
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn q_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_sets_should_quit() {
        let mut app = mem_app();
        app.handle_key(key_with(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn question_mark_opens_help() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('?')));
        assert_eq!(app.screen, Screen::Help);
    }

    #[test]
    fn esc_closes_help() {
        let mut app = mem_app();
        app.screen = Screen::Help;
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn j_moves_cursor_down() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2), ("c", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn j_clamps_at_end() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('j')));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn k_moves_up_and_saturates_at_zero() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Char('k')));
        app.handle_key(key(KeyCode::Char('k')));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn capital_g_jumps_to_last() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "open", 2), ("c", "open", 2)]);
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn l_focuses_detail_and_h_focuses_list() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('l')));
        assert_eq!(app.focus, Pane::Detail);
        app.handle_key(key(KeyCode::Char('h')));
        assert_eq!(app.focus, Pane::List);
    }

    #[test]
    fn f_then_r_toggles_ready_filter() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.pending_chord, Some('f'));
        app.handle_key(key(KeyCode::Char('r')));
        assert!(app.filters.ready_only);
        assert!(app.pending_chord.is_none());
    }

    #[test]
    fn f_then_b_toggles_blocked_filter_and_clears_ready() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.filters.ready_only = true;
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('b')));
        assert!(app.filters.blocked_only);
        assert!(!app.filters.ready_only);
    }

    #[test]
    fn f_then_c_clears_all_filters() {
        let mut app = mem_app_with(&[("a", "open", 2)]);
        app.filters.ready_only = true;
        app.filters.status = Some("open".into());
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('c')));
        assert!(app.filters.is_default());
    }

    #[test]
    fn slash_opens_text_filter_prompt() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        match &app.screen {
            Screen::FilterPrompt { kind, input } => {
                assert_eq!(*kind, FilterKind::Text);
                assert_eq!(input, "");
            }
            _ => panic!("expected FilterPrompt"),
        }
    }

    #[test]
    fn filter_prompt_typing_accumulates_into_input() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('t')));
        app.handle_key(key(KeyCode::Char('m')));
        app.handle_key(key(KeyCode::Char('u')));
        app.handle_key(key(KeyCode::Char('x')));
        match &app.screen {
            Screen::FilterPrompt { input, .. } => assert_eq!(input, "tmux"),
            _ => panic!(),
        }
    }

    #[test]
    fn filter_prompt_backspace_pops_last_char() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('a')));
        app.handle_key(key(KeyCode::Char('b')));
        app.handle_key(key(KeyCode::Backspace));
        match &app.screen {
            Screen::FilterPrompt { input, .. } => assert_eq!(input, "a"),
            _ => panic!(),
        }
    }

    #[test]
    fn filter_prompt_enter_applies_text_and_returns_to_main() {
        let mut app = mem_app_with(&[("hello world", "open", 2), ("other", "open", 2)]);
        app.handle_key(key(KeyCode::Char('/')));
        for c in "hello".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.filters.text.as_deref(), Some("hello"));
        assert_eq!(app.visible_indices().len(), 1);
    }

    #[test]
    fn filter_prompt_esc_cancels() {
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.filters.text.is_none());
    }

    #[test]
    fn applying_filter_clamps_out_of_range_cursor() {
        let mut app = mem_app_with(&[("a", "open", 2), ("b", "done", 2)]);
        app.cursor = 1;
        // Apply ready-only via chord
        app.handle_key(key(KeyCode::Char('f')));
        app.handle_key(key(KeyCode::Char('r')));
        // Only T-1 is visible (open, no deps). cursor must clamp to 0.
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn selected_task_returns_cursor_task() {
        let app = mem_app_with(&[("a", "open", 2), ("b", "open", 2)]);
        assert_eq!(app.selected_task().unwrap().title, "a");
    }

    #[test]
    fn selected_task_none_when_no_tasks() {
        let app = mem_app();
        assert!(app.selected_task().is_none());
    }
}
```

- [ ] **Step 5.3: This step intentionally cannot run yet**

The new `render()` calls `screens::main::render` which doesn't exist. We add it in Task 6. Compile is expected to fail until then.

Run: `cargo build`
Expected: FAIL with unresolved `screens::main`.

- [ ] **Step 5.4: Move on to Task 6 — do not commit here**

Both tasks 5 and 6 ship in the same commit.

---

## Task 6: Render the main screen

**Files:**
- Create: `src/tui/screens/main.rs`
- Create: `src/tui/screens/help.rs`
- Create: `src/tui/screens/filter_prompt.rs`
- Create: `src/tui/widgets/mod.rs`
- Create: `src/tui/widgets/task_list.rs`
- Create: `src/tui/widgets/task_detail.rs`
- Create: `src/tui/widgets/footer.rs`
- Create: `src/tui/widgets/top_bar.rs`
- Modify: `src/tui/mod.rs`
- Modify: `src/tui/screens/mod.rs`

- [ ] **Step 6.1: Wire widgets module**

Add to `src/tui/mod.rs`:

```rust
pub mod widgets;
```

- [ ] **Step 6.2: Wire screen submodules**

Replace `src/tui/screens/mod.rs` with:

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
pub enum Screen {
    Main,
    Help,
    FilterPrompt { kind: FilterKind, input: String },
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main
    }
}
```

- [ ] **Step 6.3: Create `src/tui/widgets/mod.rs`**

```rust
pub mod footer;
pub mod task_detail;
pub mod task_list;
pub mod top_bar;
```

- [ ] **Step 6.4: Create `src/tui/widgets/task_list.rs`**

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::{fmt_id, Task};

fn status_style(status: &str) -> Style {
    match status {
        "open" => Style::default(),
        "in_progress" => Style::default().fg(Color::Yellow),
        "done" => Style::default().fg(Color::Green).add_modifier(Modifier::DIM),
        _ => Style::default().fg(Color::Magenta),
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &[Task],
    visible: &[usize],
    cursor: usize,
    focused: bool,
) {
    let items: Vec<ListItem> = visible
        .iter()
        .map(|i| {
            let t = &tasks[*i];
            let group_suffix = t
                .group
                .as_deref()
                .map(|g| format!(" [{}]", g))
                .unwrap_or_default();
            let line = Line::from(vec![
                Span::raw(format!("{:<6}", fmt_id(t.id))),
                Span::styled(format!("{:<12}", t.status), status_style(&t.status)),
                Span::raw(format!("p{} ", t.priority)),
                Span::raw(t.title.clone()),
                Span::styled(group_suffix, Style::default().add_modifier(Modifier::DIM)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" tasks ");
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(cursor.min(visible.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}
```

- [ ] **Step 6.5: Create `src/tui/widgets/task_detail.rs`**

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::model::{fmt_id, Task};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    task: Option<&Task>,
    all_tasks: &[Task],
    focused: bool,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" detail ");

    let lines: Vec<Line> = match task {
        None => vec![Line::from(Span::styled(
            "(no task selected)",
            Style::default().add_modifier(Modifier::DIM),
        ))],
        Some(t) => {
            let mut out: Vec<Line> = Vec::new();
            out.push(Line::from(vec![
                Span::styled(fmt_id(t.id), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::raw(t.title.clone()),
            ]));
            out.push(Line::from(vec![
                Span::raw("["),
                Span::raw(t.status.clone()),
                Span::raw("]  p"),
                Span::raw(t.priority.to_string()),
            ]));
            if let Some(g) = &t.group {
                out.push(Line::from(format!("group: {}", g)));
            }
            if !t.deps.is_empty() {
                let parts: Vec<String> = t
                    .deps
                    .iter()
                    .map(|d| match all_tasks.iter().find(|x| x.id == *d) {
                        Some(dep) => format!("{} ({})", fmt_id(*d), dep.status),
                        None => format!("{} (missing)", fmt_id(*d)),
                    })
                    .collect();
                out.push(Line::from(format!("deps: {}", parts.join(", "))));
            }
            out.push(Line::from(""));
            if let Some(b) = &t.body {
                out.push(Line::from(Span::styled(
                    "## body",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for line in b.lines() {
                    out.push(Line::from(line.to_string()));
                }
                out.push(Line::from(""));
            }
            if let Some(a) = &t.acceptance {
                out.push(Line::from(Span::styled(
                    "## acceptance",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for line in a.lines() {
                    out.push(Line::from(line.to_string()));
                }
            }
            out
        }
    };

    let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(para, area);
}
```

- [ ] **Step 6.6: Create `src/tui/widgets/footer.rs`**

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::keybindings::{footer_for, Scope};

pub fn render(frame: &mut Frame, area: Rect, scope: Scope) {
    let bindings = footer_for(scope);
    let mut spans: Vec<Span> = Vec::new();
    for (i, b) in bindings.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " · ",
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
        spans.push(Span::styled(
            b.keys,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::raw(b.label));
    }
    let para = Paragraph::new(Line::from(spans));
    frame.render_widget(para, area);
}
```

- [ ] **Step 6.7: Create `src/tui/widgets/top_bar.rs`**

```rust
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::filters::Filters;

pub fn render(frame: &mut Frame, area: Rect, filters: &Filters) {
    let mut spans: Vec<Span> = vec![Span::styled(
        " docket ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    let chip_style = Style::default().fg(Color::Black).bg(Color::Cyan);

    let mut add_chip = |label: String| {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!(" {} ", label), chip_style));
    };

    if let Some(s) = &filters.status {
        add_chip(format!("status:{}", s));
    }
    if let Some(g) = &filters.group {
        add_chip(format!("group:{}", g));
    }
    if let Some(p) = filters.priority_cap {
        add_chip(format!("p≤{}", p));
    }
    if filters.ready_only {
        add_chip("ready".into());
    }
    if filters.blocked_only {
        add_chip("blocked".into());
    }
    if let Some(t) = &filters.text {
        add_chip(format!("/{}", t));
    }

    let para = Paragraph::new(Line::from(spans));
    frame.render_widget(para, area);
}
```

- [ ] **Step 6.8: Create `src/tui/screens/main.rs`**

```rust
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::tui::app::{App, Pane};
use crate::tui::keybindings::Scope;
use crate::tui::widgets::{footer, task_detail, task_list, top_bar};

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top bar
            Constraint::Min(1),    // main panes
            Constraint::Length(1), // footer
        ])
        .split(area);

    top_bar::render(frame, rows[0], &app.filters);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(20)])
        .split(rows[1]);

    let visible = app.visible_indices();
    task_list::render(
        frame,
        panes[0],
        &app.tasks,
        &visible,
        app.cursor,
        app.focus == Pane::List,
    );
    task_detail::render(
        frame,
        panes[1],
        app.selected_task(),
        &app.tasks,
        app.focus == Pane::Detail,
    );

    let scope = match app.focus {
        Pane::List => Scope::List,
        Pane::Detail => Scope::Detail,
    };
    footer::render(frame, rows[2], scope);
}
```

- [ ] **Step 6.9: Create `src/tui/screens/help.rs`**

```rust
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::keybindings::{Scope, COMMAND_REGISTRY};

pub fn render(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    for scope in [Scope::Global, Scope::List, Scope::Detail, Scope::FilterPrompt] {
        let scope_label = match scope {
            Scope::Global => "Global",
            Scope::List => "List",
            Scope::Detail => "Detail",
            Scope::Help => "Help",
            Scope::FilterPrompt => "Filter prompt",
        };
        lines.push(Line::from(Span::styled(
            scope_label.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for b in COMMAND_REGISTRY.iter().filter(|b| b.scope == scope) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<14}", b.keys),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(b.label),
            ]));
        }
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "Press ? or Esc to close",
        Style::default().add_modifier(Modifier::DIM),
    )));

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(" help "));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
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

- [ ] **Step 6.10: Create `src/tui/screens/filter_prompt.rs`**

```rust
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::screens::FilterKind;

pub fn render(frame: &mut Frame, kind: &FilterKind, input: &str) {
    let label = match kind {
        FilterKind::Status => "Filter by status",
        FilterKind::Group => "Filter by group",
        FilterKind::Priority => "Filter by priority cap (0..4, empty clears)",
        FilterKind::Text => "Filter title/body (case-insensitive substring)",
    };
    let area = centered_rect(60, frame.area());
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::from(Span::styled(
            label,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(input.to_string()),
            Span::styled("▌", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Enter apply · Esc cancel",
            Style::default().add_modifier(Modifier::DIM),
        )),
    ];

    let para = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL).title(" filter "));
    frame.render_widget(para, area);
}

fn centered_rect(percent_x: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(7),
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

- [ ] **Step 6.11: Compile**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 6.12: Run all unit tests**

Run: `cargo test`
Expected: all tests pass — App tests (~17), filters (9), keybindings (4), prior (33).

- [ ] **Step 6.13: Add a smoke render test**

Append to the `tests` module in `src/tui/app.rs`:

```rust
    #[test]
    fn main_screen_renders_top_bar_and_footer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let app = mem_app_with(&[("hello", "open", 2)]);
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect::<Vec<_>>().join("");
        assert!(s.contains("docket"));
        assert!(s.contains("hello"));
        assert!(s.contains("quit"));
    }

    #[test]
    fn help_screen_renders_on_top_of_main() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app_with(&[("hello", "open", 2)]);
        app.screen = Screen::Help;
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect::<Vec<_>>().join("");
        assert!(s.contains("help"));
        assert!(s.contains("Press ? or Esc"));
    }

    #[test]
    fn filter_prompt_renders() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let mut app = mem_app();
        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('h')));
        app.handle_key(key(KeyCode::Char('i')));
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let s: String = buf.content().iter().map(|c| c.symbol()).collect::<Vec<_>>().join("");
        assert!(s.contains("Filter title/body"));
        assert!(s.contains("hi"));
    }
```

- [ ] **Step 6.14: Run the new tests**

Run: `cargo test tui::app::tests`
Expected: all pass (~20 tests in the `app::tests` module).

- [ ] **Step 6.15: Run the full suite**

Run: `cargo test`
Expected: ~63 tests pass.

- [ ] **Step 6.16: Commit Tasks 5+6 together**

```bash
git add src/tui/app.rs src/tui/mod.rs src/tui/screens/ src/tui/widgets/
git commit -m "feat(tui): main screen with list/detail panes, filters, help, and chord-driven prompts"
```

---

## Task 7: Manual smoke test

- [ ] **Step 7.1: Seed the local `.docket/` with a few tasks**

If you haven't added test tasks to your repo's docket store, do so now:

```bash
cargo run -- add "Sample TUI task" --body "Some body text" --acceptance "It opens" --priority 1
cargo run -- add "Another one" --body "More body" --priority 2
cargo run -- add "Third with deps" --deps T-1,T-2 --priority 3
```

- [ ] **Step 7.2: Launch the TUI**

```bash
cargo run -- tui
```

Expected: two-pane view, list on the left with three rows, detail on the right showing the cursor task. Top bar shows "docket". Footer shows "? help · q quit · / filter text · j/k nav · l detail · f r ready · f c clear filters".

- [ ] **Step 7.3: Verify navigation**

Press `j` twice — cursor moves to row 3 in the list, detail pane updates. Press `k` — back to row 2. Press `G` — last row. Press `g` — first row. Press `l` — right-pane border highlights cyan; press `h` — back to left.

- [ ] **Step 7.4: Verify filters**

Press `f` then `r` — top bar gets a "ready" chip; list updates. Press `f` then `c` — chip clears.
Press `/`, type `tui`, Enter — top bar shows `/tui` chip, list narrows. Press `/` again, Enter on empty input — chip clears.
Press `f` then `p`, type `2`, Enter — only priority ≤ 2 tasks remain.

- [ ] **Step 7.5: Verify help and quit**

Press `?` — overlay appears. Press `Esc` — overlay closes. Press `q` — TUI exits, shell prompt returns.

No commit (no files changed). If anything misbehaves, return to the relevant task.

---

## Self-review

**Spec coverage** (against §4 of the design doc):
- §4 two-pane layout — Task 6 (main.rs, task_list, task_detail)
- §4.1 scopes — Task 3 (keybindings.rs Scope enum); applied in main screen render (Task 6)
- §4.2 keybindings: q/?/Ctrl+C — Task 5; j/k/g/G/l/h — Task 5; / and R — Task 5; f s/g/p — Task 5 (opens prompt); f r/b/c — Task 5 (in-place toggles); PgUp/PgDn — listed in registry, scroll state deferred (acceptable for read-only PR; body wraps within detail box anyway)
- §4.3 decisions: filter chord chosen — Task 5; status cycle / start integration — NOT in scope (PR-4 / PR-6)
- Top-bar filter chips — Task 6 (top_bar.rs)
- Contextual footer — Task 6 (footer.rs)

**Placeholder scan:** no TBDs; every step has runnable code or commands.

**Type consistency:**
- `Filters` struct fields (`status`, `group`, `priority_cap`, `ready_only`, `blocked_only`, `text`) — referenced consistently across `filters.rs`, `app.rs`, and `top_bar.rs`.
- `Screen` enum is defined twice — Task 4 creates a placeholder, Task 6 replaces with the final version including submodule declarations. The final version is what compiles.
- `App` is grown in Task 5; the field set must match the rendering code in Task 6 (it does — `tasks`, `filters`, `cursor`, `focus`, `screen`, `selected_task()`, `visible_indices()`).
- `Pane` enum (`List` / `Detail`) — defined in `app.rs` Task 5, used in `screens/main.rs` Task 6.

**Out of scope (intentionally deferred to PR-4+):**
- Status cycling (`s` / `Shift+s`)
- Mark done (`d`)
- Delete (`x`) with confirm modal
- Start integration (`S` / `Ctrl+s`)
- Add / Edit form
- Detail-pane scroll state (the registry advertises PgUp/PgDn; scroll-on-keypress is a small follow-up if the body is long enough to need it)
