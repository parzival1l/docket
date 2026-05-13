# Docket TUI — Design Spec

**Date:** 2026-05-13
**Status:** Approved (sections 1–2); sections 3–4 captured here for review.
**Owner:** parzival1l
**Related reference:** `~/Personal/threadhop/threadhop_core/tui/` (Textual TUI; we mirror its *patterns*, not its language).

## 1. Goal & scope

Add a Rust TUI to docket — `docket tui` — that provides **full task management** over the existing SQLite store: browse, add, edit, status changes, delete, and start. The TUI is a peer of the existing CLI; both consume the same internal `db` / `model` modules.

**In scope (day one):**

- Two-pane list + detail browser with filters
- Add and edit forms covering all task fields, including multi-line `body` and `acceptance` via an **inline TUI text area** (no `$EDITOR` shellout)
- Status cycling, mark-done, delete (with confirm)
- `start` and `start --tmux` parity from inside the TUI
- Filter chips: status, group, priority cap, ready-only, blocked-only, free-text
- Help overlay (`?`) driven by the same command registry that drives the contextual footer

**Out of scope (day one):**

- Group CRUD beyond filtering by group (groups remain CLI-managed for now)
- Multi-task selection / bulk operations
- Mouse support, syntax-highlighted markdown, themes beyond a single default
- Watch-mode / auto-refresh from external DB changes

## 2. Approaches considered

| | Approach | Decision |
|---|---|---|
| A | Single binary, internal modules — extract `src/main.rs` into `db`/`model`/`cli`/`tui` modules, add `tui` subcommand | **Chosen.** Smallest viable refactor; matches threadhop's "one package, internal modules" pattern. |
| B | Cargo workspace (`docket-core` + `docket-cli` + `docket-tui`) | Rejected. Premature for ~1500 LOC; adds release + Cargo.toml overhead with no current benefit. |
| C | TUI as a separate binary that shells out to `docket` for every action | Rejected. Subprocess per keystroke and per SQLite open; fragile. |

## 3. Architecture

### 3.1 Module layout (post-refactor)

```
src/
  main.rs            # thin: clap parse → dispatch to cli or tui
  model.rs           # Task, Group, parse_id, parse_deps, deps_from_db, fmt_id
  db.rs              # open_db, init schema, load_all_tasks, get_or_create_group,
                     # insert_task, update_task, set_status, delete_task
  prompts.rs         # include_str! template constants + assemble_prompt
  cli/
    mod.rs           # dispatch from clap subcommands
    add.rs, ls.rs, show.rs, ready.rs, blocked.rs,
    status.rs, rm.rs, prompt.rs, start.rs, group.rs   # one verb per file
  tui/
    mod.rs           # run_tui(): event loop, terminal setup/teardown
    app.rs           # App state machine
    keybindings.rs   # COMMAND_REGISTRY (Scope, keys, label, action, footer)
    theme.rs         # color palette
    screens/
      main.rs        # list + detail two-pane
      edit.rs        # add/edit form
      help.rs        # help overlay
      confirm.rs     # delete confirm
      filter.rs      # quick filter prompts
    widgets/
      task_list.rs
      task_detail.rs
      footer.rs      # contextual footer driven by COMMAND_REGISTRY
```

### 3.2 Crates

| Crate | Purpose |
|---|---|
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend (cross-platform, already pulled by ratatui) |
| `tui-textarea` | Multi-line editor widget for `body` / `acceptance` |
| `tui-input` | Single-line editor widget for `title`, `priority`, `group`, `deps`, filter prompts |

### 3.3 Entrypoint

`docket tui` — a new clap subcommand. Bare `docket` continues to print clap help. No default behavior change.

### 3.4 State ownership

`App` owns:

- `Connection` (rusqlite)
- `Vec<Task>`, `Vec<Group>` (snapshot loaded on startup and after every mutation)
- `Filters` (status, group, priority cap, ready/blocked toggles, text search)
- `Selection { cursor: usize, focus: Pane }`
- `Screen` enum (current screen drives both render and event routing)
- `StatusLine { message: String, level: Info|Error, expires_at: Instant }` for transient feedback

**Refresh discipline:** every mutation goes through a `db::*` function that returns the affected row count; on success the App calls `reload()` to refetch `Vec<Task>`. No client-side cache invalidation logic. The dataset is tiny — re-querying is cheap and removes a whole class of bugs.

### 3.5 Screen state machine

```rust
enum Screen {
    Main,                       // list + detail
    Edit(EditState),            // add or edit (EditState.task_id: Option<i64>)
    Confirm(PendingAction),     // delete confirm
    Help,                       // overlay
    Filter(FilterKind),         // single-line prompt for filter values
}
```

Modals (`Edit`, `Confirm`, `Help`, `Filter`) render on top of `Main`. Key events route to the topmost screen; only `Main` events reach the list/detail panes.

## 4. Main screen — layout & keys

```
┌ docket ──────────────────────── [open] [group:all] [p≤2] [ready] ─┐
│ T-5  open    p2  Hermetic tes…│ T-5  Hermetic test suite for…    │
│ T-6  open    p2  AGENTS.md: d…│ [open]  p2                       │
│▌T-9  open    p2  schema: add …│ group: v0.0.2                    │
│ T-10 open    p2  lifecycle: b…│ deps: T-2 (done), T-3 (done)     │
│ T-4  in_pro… p3  v0.0.4+: --i…│                                  │
│                               │ ## body                          │
│                               │ Lorem ipsum dolor sit amet…      │
│                               │                                  │
│                               │ ## acceptance                    │
│                               │ - docket ls --kind=bug works     │
└───────────────────────────────┴──────────────────────────────────┘
 ? help · j/k nav · enter detail · n new · e edit · s status · S start
```

### 4.1 Scopes

`global`, `list`, `detail`, `edit_form`, `confirm`, `help`, `filter_prompt`.

### 4.2 Keybindings (Main + global)

| Scope | Key | Action |
|---|---|---|
| global | `?` | open help overlay |
| global | `q` | quit (confirm if Edit screen is dirty) |
| global | `/` | open free-text filter prompt (case-insensitive substring match on title + body) |
| global | `R` | reload from DB |
| list | `j` / `k` | next / prev task |
| list | `g` / `G` | jump to top / bottom |
| list | `l` / `→` | focus detail pane |
| list | `Enter` | toggle detail to full-width |
| list | `n` | open Add form |
| list | `e` | open Edit form for cursor task |
| list | `x` | delete → Confirm modal |
| list | `s` / `Shift+s` | cycle status forward / backward (open → in_progress → done) |
| list | `d` | mark cursor task done |
| list | `S` | **start task**: suspend TUI, print assembled prompt to stdout, mark `in_progress`, exit (parity with `docket start T-N`) |
| list | `Ctrl+s` | start with tmux delivery (parity with `docket start T-N --tmux`) |
| list | `f s` | chord: filter by status |
| list | `f g` | chord: filter by group |
| list | `f p` | chord: filter by priority cap |
| list | `f r` | toggle ready-only |
| list | `f b` | toggle blocked-only |
| list | `f c` | clear all filters |
| detail | `h` / `←` | focus list |
| detail | `e` | edit current task |
| detail | `PgUp`/`PgDn`/`Home`/`End` | scroll body |

### 4.3 Decisions captured

- **`S` exits the TUI after printing the prompt.** Matches `docket start` semantics so the prompt can be piped/captured the same way. Alternative ("copy to clipboard, stay in TUI") rejected because it diverges from CLI behaviour and introduces a clipboard dependency.
- **Filters use a chord (`f <letter>`)**, not single keys, because single-letter filter keys collide with status cycling (`s`) and other list verbs. The chord stays out of the way and is easy to extend (`f k` for `kind` once T-9 lands).
- **`d` is a shortcut for "set status = done"**, not "delete." Delete is `x` (a destructive single-letter convention borrowed from vim). The Confirm modal prevents accidents.
- **Status cycle only walks the canonical three** (`open`, `in_progress`, `done`); custom statuses entered via CLI are visible but not part of the cycle.
- **Top-bar filter chips are display-only.** All filter mutation goes through `f` chords or `/`. Keeps a single input model.

## 5. Edit / Add form

### 5.1 Layout

A centered modal, fixed width (~80 cols if terminal allows), variable height. Fields top-to-bottom:

```
┌ Edit T-9 ───────────────────────────────────────────────────────┐
│ Title       [ schema: add 'kind' column for task categoriz…   ] │
│ Priority    [ 2 ]                                               │
│ Group       [ v0.0.2 ]                                          │
│ Deps        [ T-5, T-6 ]                                        │
│ Body        ┌─────────────────────────────────────────────────┐ │
│             │ Add a kind enum (bug|feature|chore|docs|spike). │ │
│             │ Default to feature on existing rows.            │ │
│             │ ▌                                               │ │
│             └─────────────────────────────────────────────────┘ │
│ Acceptance  ┌─────────────────────────────────────────────────┐ │
│             │ - docket ls --kind=bug filters to bugs          │ │
│             │ - migration is idempotent                       │ │
│             └─────────────────────────────────────────────────┘ │
│                                                                 │
│  Tab/Shift+Tab next/prev · Ctrl+S save · Esc cancel             │
└─────────────────────────────────────────────────────────────────┘
```

- **Title / Priority / Group / Deps:** `tui-input` single-line fields.
- **Body / Acceptance:** `tui-textarea` widgets. `Enter` inserts a newline inside these; navigation between fields is `Tab` / `Shift+Tab`.
- **Deps** parses identical to `docket add --deps` (comma- or space-separated `T-N` or bare ints).

### 5.2 Keybindings (edit_form scope)

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | next / prev field |
| `Ctrl+S` | validate + save |
| `Esc` | cancel (confirm discard if dirty) |
| (within textarea) `Enter` | newline |
| (within textarea) standard editing keys | move/insert/delete as the widget provides |

### 5.3 Validation

Performed on save, all errors surfaced in a status line at the bottom of the modal (no field-level error chrome day one):

- `title` — required, non-empty after trim
- `priority` — must parse as `i32` in `0..=4`; empty → default `2` (matches CLI)
- `group` — any string; empty → unset; non-empty → lazy-create via `get_or_create_group`
- `deps` — every token must parse to a valid task id; missing referenced ids are accepted (matches CLI semantics, which let `show` flag them as `(missing)`)
- `body`, `acceptance` — free text; empty → stored as `NULL`

### 5.4 Save semantics

- **Add:** `db::insert_task(...)` → reload → close modal → cursor jumps to new task.
- **Edit:** `db::update_task(id, ...)` → reload → close modal → preserve cursor on edited task.
- On DB error: leave modal open, populate status line with error message, do not clear inputs.

### 5.5 Dirty-state tracking

`EditState` keeps the original snapshot. `Esc` and `q` prompt to discard if any field diverges from the snapshot. Avoids the "lost a paragraph of acceptance criteria" foot-gun.

## 6. Data flow

1. **Startup:** parse `Cli`, dispatch to `tui::run_tui()`. `run_tui` opens the DB (errors propagate normally), constructs `App`, loads tasks + groups, enters the event loop.
2. **Render frame:** `App::render(frame)` selects screen renderer; Main delegates to `task_list` + `task_detail`; modals overlay.
3. **Event tick:** `crossterm::event::poll(50ms)` → on key, dispatch to `App::handle_key(key)` which routes via current `Screen` and focused pane.
4. **Mutation:** key handler calls `db::*` → on `Ok`, calls `App::reload()` → on `Err`, sets status line.
5. **Shutdown:** restore terminal (leave alt screen, disable raw mode) on every exit path — including panic. Implemented via a guard struct in `tui::mod` that runs cleanup in `Drop`.

## 7. Error handling

- **DB errors during mutation:** captured into the status line; modal/screen stays open so the user can retry or cancel.
- **DB errors during initial load:** propagate out of `run_tui`; `main` prints the anyhow chain and exits non-zero (same as today's CLI).
- **Panics inside the event loop:** caught by the terminal-restore `Drop` guard so the user's shell isn't left in raw mode. The panic itself still terminates the process — we do not try to recover.
- **`start` failure modes:** when `S` is pressed inside tmux without `Ctrl+s`, prompt is printed and TUI exits. When `Ctrl+s` is pressed outside tmux, status line shows the same error today's `docket start --tmux` returns (`not inside a tmux session`), and the TUI stays open.

## 8. Testing

- **`db.rs` (new):** unit tests on every query/mutation using in-memory `:memory:` SQLite connections. Mirrors how `tests/start.rs` works today.
- **`model.rs`:** unit tests for `parse_id`, `parse_deps`, `deps_from_db` round-trip.
- **`tui` event handling:** drive `App::handle_key` with synthetic `KeyEvent`s and assert on resulting `App` state (no rendering). Covers the chord state machine, status cycling, screen transitions, dirty-state guards.
- **`tui` rendering smoke tests:** use ratatui's `TestBackend` to render a single frame and snapshot expected strings (cursor row marker, filter chip text, modal title). Light-weight goldens, not pixel-perfect snapshots.
- **CLI regression:** existing `tests/start.rs` must still pass after the refactor — proves the `db`/`model` extraction didn't change behaviour.

## 9. Refactor sequencing

To keep the diff reviewable and avoid the "huge unreviewable PR" failure mode, ship in this order:

1. **Module extraction** with no behaviour change: split `src/main.rs` into `model.rs`, `db.rs`, `prompts.rs`, `cli/*.rs`. Existing tests pass unchanged. One PR.
2. **TUI skeleton**: add `tui::run_tui`, `docket tui` subcommand, raw-mode setup/teardown, empty App that quits on `q`. Proves the wiring. One PR.
3. **Main screen read-only**: list + detail + filters + help overlay. No mutations. One PR.
4. **Mutations**: status cycle, mark-done, delete (confirm). One PR.
5. **Edit / Add form**: full form with `tui-textarea`, validation, dirty tracking. One PR.
6. **Start integration**: `S` and `Ctrl+s` from list. One PR.

Each step is independently shippable and each adds tests.

## 10. Open / deferred questions

- Should `Enter` on the list also offer a "copy id to clipboard" affordance? Deferred — there's no clipboard crate dependency yet, and `g` (vim "goto top") is already taken. Revisit if a user asks.
- Should the TUI expose group CRUD? Deferred — groups are stable infrastructure today, mutated rarely.
- Theme support beyond a hardcoded palette? Deferred until docket has more than one user with preferences.
