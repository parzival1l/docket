# Docket TUI — PR-2: Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a minimal `docket tui` subcommand that opens a ratatui terminal, renders a placeholder frame, and exits cleanly on `q` or `Ctrl+C` — proving the wiring (deps, entrypoint, terminal lifecycle, event loop, panic-safe teardown) without committing to any task views yet.

**Architecture:** Add `ratatui` + `crossterm` deps. Wire a new `Tui` variant in the clap `Command` enum to `cli::tui::run`, which constructs an `App` and runs the event loop in `tui::run_tui`. Terminal mode setup/teardown is handled by a RAII guard (`TerminalGuard` in `tui::mod`) so `Drop` runs even on panic — your shell never gets stranded in raw mode.

**Tech Stack:** Rust 2021, `ratatui` 0.29, `crossterm` 0.28 (bundled). No `tui-textarea` / `tui-input` yet (lands in PR-5 with the edit form).

---

## File Structure (delta from PR-1)

```
src/
  cli/
    mod.rs           # Modify: declare `pub mod tui;`, add `Command::Tui`, dispatch arm
    tui.rs           # Create: thin CLI handler — `pub fn run() -> Result<()>` that calls tui::run_tui
  tui/
    mod.rs           # Create: run_tui(), TerminalGuard (RAII), event loop
    app.rs           # Create: App { should_quit: bool }, handle_key, render
```

`src/cli/tui.rs` is intentionally distinct from `src/tui/mod.rs` — `cli::tui` is the clap-side handler (one file per verb, matching every other verb), `tui::` is the TUI implementation it delegates into.

---

## Task 1: Add ratatui + crossterm dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1.1: Add the deps to `Cargo.toml`**

In the `[dependencies]` block, add:

```toml
ratatui = "0.29"
crossterm = "0.28"
```

The resulting block should look like:

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
```

- [ ] **Step 1.2: Resolve and cache**

Run: `cargo build`
Expected: clean build, new crates downloaded.

- [ ] **Step 1.3: Verify existing tests still pass**

Run: `cargo test`
Expected: 39 tests pass (28 unit + 11 integration).

- [ ] **Step 1.4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add ratatui and crossterm for TUI"
```

---

## Task 2: Add `Tui` command variant to clap

**Files:**
- Modify: `src/cli/mod.rs` — add the variant and the dispatch arm

- [ ] **Step 2.1: Add `Tui` to the `Command` enum**

Open `src/cli/mod.rs`. Inside `pub enum Command { ... }`, add a new variant after `Group`:

```rust
    /// Open the interactive TUI
    Tui,
```

- [ ] **Step 2.2: Declare the submodule and the dispatch arm**

Near the other `pub mod` lines in `src/cli/mod.rs`, add (alphabetically):

```rust
pub mod tui;
```

In the `match cli.command { ... }` body, add an arm before the closing brace:

```rust
        Command::Tui => tui::run(),
```

- [ ] **Step 2.3: Compile (will fail — `tui` module doesn't exist yet)**

Run: `cargo build`
Expected: FAIL with `unresolved module or unlinked crate: tui` — that's correct; we land the module in Task 3.

---

## Task 3: Create the `cli::tui` handler

**Files:**
- Create: `src/cli/tui.rs`

- [ ] **Step 3.1: Write `src/cli/tui.rs`**

```rust
use anyhow::Result;

pub fn run() -> Result<()> {
    crate::tui::run_tui()
}
```

That's the entire file. The CLI handler keeps zero TUI logic — it's just the seam between clap and the TUI implementation.

- [ ] **Step 3.2: Try to compile (will fail — `crate::tui` module doesn't exist yet)**

Run: `cargo build`
Expected: FAIL with `unresolved module or unlinked crate: tui` (now pointing at `crate::tui`). Task 4 fixes this.

---

## Task 4: Create the `tui` module with App + event loop

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`
- Modify: `src/main.rs` — declare `mod tui;`

- [ ] **Step 4.1: Write `src/tui/app.rs`**

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

#[derive(Default)]
pub struct App {
    pub should_quit: bool,
}

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) => self.should_quit = true,
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            _ => {}
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let title = Line::from(Span::styled(
            " docket ",
            Style::default().add_modifier(Modifier::BOLD),
        ));
        let body = Paragraph::new(vec![
            Line::from(""),
            Line::from("TUI skeleton — coming soon."),
            Line::from(""),
            Line::from(Span::styled(
                "Press q or Ctrl+C to quit",
                Style::default().add_modifier(Modifier::DIM),
            )),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(body, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEventKind;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn q_sets_should_quit() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_c_sets_should_quit() {
        let mut app = App::default();
        app.handle_key(key_with(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn plain_c_does_not_quit() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Char('c')));
        assert!(!app.should_quit);
    }

    #[test]
    fn unhandled_keys_are_ignored() {
        let mut app = App::default();
        app.handle_key(key(KeyCode::Enter));
        app.handle_key(key(KeyCode::Char('x')));
        assert!(!app.should_quit);
    }

    #[test]
    fn render_does_not_panic_on_test_backend() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = App::default();
        terminal.draw(|f| app.render(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let rendered: String = buffer
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(rendered.contains("docket"));
        assert!(rendered.contains("Press q"));
    }
}
```

- [ ] **Step 4.2: Write `src/tui/mod.rs`**

```rust
use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::Duration;

pub mod app;

use app::App;

/// RAII guard that restores the terminal to a usable state when dropped,
/// even if the event loop panics. Without this, a panic during render or
/// event handling leaves the shell in raw mode + alt screen.
struct TerminalGuard {
    inner: Option<Terminal<CrosstermBackend<Stdout>>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen).context("enter alt screen")?;
        let backend = CrosstermBackend::new(out);
        let terminal = Terminal::new(backend).context("create terminal")?;
        Ok(Self {
            inner: Some(terminal),
        })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        self.inner.as_mut().expect("terminal taken")
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Best-effort cleanup. We deliberately swallow errors here so a
        // panic mid-draw can't double-panic in Drop.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

pub fn run_tui() -> Result<()> {
    let mut guard = TerminalGuard::new()?;
    let mut app = App::default();

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

- [ ] **Step 4.3: Declare the `tui` module in `src/main.rs`**

Open `src/main.rs`. Add `mod tui;` in the existing module-declaration block. The whole file becomes:

```rust
use anyhow::Result;
use clap::Parser;

mod cli;
mod db;
mod model;
mod prompts;
mod tui;

fn main() -> Result<()> {
    let args = cli::Cli::parse();
    cli::dispatch(args)
}
```

- [ ] **Step 4.4: Compile**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 4.5: Run the new unit tests**

Run: `cargo test tui::app::tests`
Expected: 5 tests pass (`q_sets_should_quit`, `ctrl_c_sets_should_quit`, `plain_c_does_not_quit`, `unhandled_keys_are_ignored`, `render_does_not_panic_on_test_backend`).

- [ ] **Step 4.6: Run the full suite**

Run: `cargo test`
Expected: 44 tests pass (28 prior unit + 5 new tui + 11 integration).

- [ ] **Step 4.7: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/cli/mod.rs src/cli/tui.rs src/tui/mod.rs src/tui/app.rs
git commit -m "feat: add docket tui subcommand skeleton"
```

---

## Task 5: Manual smoke test

The unit tests cover key handling and render-doesn't-panic, but the terminal lifecycle (raw mode, alt screen, restore-on-quit, restore-on-panic) can only be verified interactively.

- [ ] **Step 5.1: Launch the TUI**

In a real terminal (not a CI runner), run:

```bash
cargo run -- tui
```

Expected: terminal enters alt screen, a bordered panel reads "docket TUI skeleton — coming soon · Press q or Ctrl+C to quit".

- [ ] **Step 5.2: Quit with `q`**

Press `q`.

Expected: the alt screen exits, raw mode is disabled, the shell prompt reappears with no garbage characters, exit code is `0`.

- [ ] **Step 5.3: Re-launch and quit with `Ctrl+C`**

```bash
cargo run -- tui
```

Press `Ctrl+C`.

Expected: same clean exit as `q`. (We intentionally treat `Ctrl+C` as a quit key inside the TUI — crossterm delivers it as a key event in raw mode, not as a SIGINT.)

- [ ] **Step 5.4: Verify the existing `docket` commands still work**

Run a few of the existing verbs to confirm the new clap variant didn't break parsing:

```bash
cargo run -- --help | grep -i tui
cargo run -- ls --help
```

Expected: `--help` lists `tui` as a subcommand; `ls --help` works as before.

No commit needed for the manual smoke (no files changed). If anything misbehaves, return to the relevant earlier task and fix.

---

## Self-review (mental, before handoff)

**Spec coverage** (against §3.2 / §3.3 / §3.4 / §6 / §7 of the design doc):
- §3.2 crates — Task 1 adds `ratatui` + `crossterm`; `tui-textarea` / `tui-input` deliberately deferred to PR-5
- §3.3 entrypoint — Task 2 wires `docket tui` as a clap subcommand
- §3.4 App owns connection / tasks / filters / screen — only `should_quit` lands here; loading from DB is PR-3 work
- §6 data flow (event tick at 50–100ms) — `event::poll(Duration::from_millis(100))` in Task 4
- §7 panic-safe teardown — `TerminalGuard` Drop in Task 4

**Placeholder scan:** no TBDs; every step has runnable code or a runnable command with expected output.

**Type consistency:** `App` exposed as `pub struct App { pub should_quit: bool }` with `handle_key(&mut self, KeyEvent)` and `render(&self, &mut Frame)`. PR-3 extends `App` (filters, screens, tasks) — these signatures are forward-compatible.

**Out of scope (intentionally deferred):**
- Any list / detail rendering (PR-3)
- Loading tasks from the DB into App (PR-3)
- `tui-textarea` / `tui-input` (PR-5)
- The `S` / `Ctrl+s` start integration (PR-6)
