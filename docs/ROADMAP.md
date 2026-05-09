# Roadmap

Tracked deferrals from the v1 design pass. Items live here so they don't drift out of conversation history.

## Shipped

- [x] **Release plumbing** — tag-triggered GitHub Actions workflow at `.github/workflows/release.yml`, cross-builds for `{macOS, Linux} × {x86_64, aarch64}`, publishes a GitHub Release with SHA-256 checksums and notes pulled from `CHANGELOG.md`. `install.sh` handles `curl | bash` install with checksum verification.

## Next iteration (towards 0.1.0)

Versioning is `0.0.x` — every shipped item bumps the patch (0.0.2, 0.0.3, …). The cut to `0.1.0` is **maintainer judgment, not a feature checklist**: it happens when the loop is dogfood-proven and the tool feels solid for daily use. `docket start` landing is the most likely trigger but is not by itself sufficient.

- [ ] **`docket start <task>`** — single-task spawn. Opens a fresh Claude session with the task body + acceptance + `tdd-pursuit` prompt injected. Strictly simpler than `docket work` — single task, no dep ordering, no group-completion check. The load-bearing differentiator vs. beads.

  **Phased build plan** (each phase ships as its own version bump):

  - **Phase 1 — stdout MVP.** `docket start T-N` looks up the task, builds the prompt (`# Task T-N: <title>` + body + acceptance + the embedded `tdd-pursuit` prompt verbatim), flips status to `in_progress`, and prints the assembled prompt to stdout. ~40 lines in `main.rs`. No multiplexer code, no spawning. Composes with everything: `docket start T-1 | claude`, `docket start T-1 | pbcopy`, `docket start T-1 > /tmp/p.md`. Unblocks the loop in one afternoon.
  - **Phase 2 — `--tmux` flag.** Default behavior stays stdout. `docket start T-N --tmux` opens a new window in the current tmux session and pipes the prompt into a fresh `claude` invocation there. Detects via `$TMUX`; uses `tmux new-window` + `send-keys`.
  - **Phase 3+ — `--iterm`, `--zellij`, `--wezterm`.** Add as the friction shows up; each is a self-contained code path that doesn't touch the others.

  Don't auto-detect the multiplexer in Phases 1 or 2 — explicit flag keeps the stdout default fast and predictable.
- [ ] **`docket work <group>`** — the sequential execution loop, building on `docket start`. Spawns a fresh Claude session **in a new tmux window** per task in dep order, single branch, one PR at end. Each spawned session receives the task body + acceptance + the `tdd-pursuit` prompt. The mechanism is decided (tmux + Claude); the implementation work is deferred. This is the load-bearing differentiator vs. beads.
- [ ] **Tests for `docket` itself** — use its own TDD harness on itself. Cargo + `assert_cmd` + `tempfile` for end-to-end coverage of every CLI verb.
- [ ] **`AGENTS.md` generation on `docket init`** — write a small doc to repo root describing the `[T-N]` commit convention, the `docket ready` loop, and the four prompts so any agent landing in the repo gets oriented without us having to brief it.
- [ ] **`docket update` / `changelog` / `future` subcommands** — port from threadhop's surface. `update` checks GitHub Releases and self-replaces (`self_update` crate); `changelog` and `future` print embedded `CHANGELOG.md` / top-N `ROADMAP.md` entries. Companion: 24h update-check nudge on next CLI invocation, suppressed in pipelines and when `DOCKET_NO_UPDATE_CHECK=1`.
- [ ] **`audit` prompt** — placeholder mentioned during design. Two candidate roles surfaced: (1) **post-hoc anti-cheat sniff** — review the diff and tests for evidence of any of the five named cheats from `tdd-pursuit.md`; (2) **planning-time validation** — verify that acceptance criteria are testable before `docket add` actually inserts the task. Decide which role it plays (they're different prompts) before writing.

## Later (deferred from v1 design)

- [ ] **Per-repo prompt overrides** — let a repo ship its own `tdd-pursuit.md` etc. via a `.docket-prompts/` (tracked) folder, falling back to the binary's embedded defaults. Don't build until friction shows up.
- [ ] **Export / import** — `docket export > board.jsonl`, `docket import board.jsonl`. Lets boards travel across machines without committing the SQLite file.
- [ ] **Cross-repo aggregation** — global `docket` view across all `.docket/` folders on the machine. Useful when juggling multiple repos.
- [ ] **Optional Postgres sync** — for shared team boards. Originally framed as a *company-level Postgres / PG Admin rollup* with one table per project, so other developers can see each other's boards without sync ceremony. Same JSONL export becomes the wire format. Multi-user implies adding `assignee` and `audit` columns at that point — not before.
- [ ] **TUI** — Textual-style kanban view. Mentioned in early design; deferred indefinitely. The CLI + `--json` covers most cases; build a TUI only if you find yourself wanting one repeatedly.

## Open questions parked from design

- **Auto-close groups when all tasks done, or always manual `docket group close`?** Currently manual. Revisit if it feels like ceremony.
- **Status validation.** `docket status` accepts any string today. If usage shows people typing `Done` vs `done` and getting stuck, add a normalization layer or an enum check.
- **`completion_notes` column on tasks.** Discussed during design, not added. If `tdd-pursuit` exits with a one-line summary worth persisting, this is where it goes.

## Out of scope (the line docket refuses to cross)

`docket` will not grow into a workflow engine. No formulas, no DSLs, no `docket cook`, no `docket swarm`, no parallel orchestration. Orchestration belongs in the shell that calls `docket ready`. The moment we add a TOML formula format we've stopped being a SQLite task list with a TDD harness and started competing with Temporal, Prefect, and beads — without their staffing.

If a feature request feels like it belongs here, it probably belongs in a separate tool that *uses* `docket` underneath.
