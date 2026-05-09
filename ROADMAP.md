# Roadmap

Tracked deferrals from the v1 design pass. Items live here so they don't drift out of conversation history.

## v0.2 — next iteration

- [ ] **`docket start <task>`** — single-task spawn. Auto-detects terminal multiplexer (tmux / zellij / wezterm / iTerm) and opens a fresh Claude session in a new window/tab with `docket show T-N --as-prompt` injected. Strictly simpler than `docket work` — single task, no dep ordering, no group-completion check. Probably the right v0.2 first step.
- [ ] **`docket work <group>`** — the sequential execution loop, building on `docket start`. Spawns a fresh Claude session **in a new tmux window** per task in dep order, single branch, one PR at end. Each spawned session receives the task body + acceptance + the `tdd-pursuit` prompt. The mechanism is decided (tmux + Claude); the implementation work is deferred. This is the load-bearing differentiator vs. beads.
- [ ] **Tests for `docket` itself** — use its own TDD harness on itself. Cargo + `assert_cmd` + `tempfile` for end-to-end coverage of every CLI verb.
- [ ] **`AGENTS.md` generation on `docket init`** — write a small doc to repo root describing the `[T-N]` commit convention, the `docket ready` loop, and the four prompts so any agent landing in the repo gets oriented without us having to brief it.
- [ ] **Release plumbing** — GitHub Releases, cross-compile via CI (Linux x86_64/aarch64, macOS x86_64/aarch64), `curl | sh` install script.
- [ ] **`audit` prompt** — placeholder mentioned during design. Two candidate roles surfaced: (1) **post-hoc anti-cheat sniff** — review the diff and tests for evidence of any of the five named cheats from `tdd-pursuit.md`; (2) **planning-time validation** — verify that acceptance criteria are testable before `docket add` actually inserts the task. Decide which role it plays (they're different prompts) before writing.

## v0.3+ — deferred from design

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
