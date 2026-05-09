# Research Notes — docket

Design and research dump from the v1 sessions. Captured here so the conversation context isn't the only place this lives. Reads by topic, not strictly chronological.

## 1. Origin and premise

### Origin

The session began with the user trying to use Symphony's `WORKFLOW.md` + Linear flow for agent-driven coding tasks, then exploring Cline / Kanban-shaped tools, then trying to bend Asana to fit. Each failed at the same point: the *task primitives* in those tools were PM-shaped (assignees, due dates, prose descriptions), not agent-shaped (testable acceptance, blocked-by, fresh-context spawn). User's stated goal:

> "I want this to complement the other tools that we have in the software development lifecycle, like GitHub, etc., not try to overwrite these things."

`docket` exists because Symphony+Linear was too heavy, Asana's task primitives didn't fit agent ergonomics, and Cline/Kanban-style flows kept assuming a human-driven UI rather than an agent-callable interface.

### Premise

**The product is a TDD execution harness that happens to need a task store**, not a task tracker. The task store is a means to the end of disciplined agent execution. This framing matters because it tells us where to stop building — at the smallest task store that supports the harness, not at "the best task tracker."

What an agent picks up should:

- Be defined precisely enough to execute against (testable `acceptance`)
- Sit next to other tasks in a way the agent can navigate (`deps`, `groups`, `ready`)
- Travel cleanly across sessions (per-repo SQLite, no in-conversation memory)
- Not require ceremony to use (`docket add` from chat, `docket ready` to pick up)

This drives every decision below.

## 2. Reference points consulted

- **`/Users/nandakumar/Personal/symphony/`** — Elixir/Phoenix orchestrator. `.codex/skills/<name>/SKILL.md` pattern with YAML frontmatter + Goals/Inputs/Steps body.
- **`/Users/nandakumar/Personal/kanban/`** — front-end coding agent project. Procedural-first prompt style: numbered steps, "Before/During/After" checklists, no frontmatter.
- **mattpocock TDD skills** (https://github.com/mattpocock/skills/tree/main/skills/engineering/tdd) — six files covering RED-GREEN-REFACTOR, test design, refactoring catalog, mocking strategy, deep-modules.
- **beads** (https://github.com/gastownhall/beads) — 23k-star agent task tracker, Go binary, Dolt-backed, mature. Has `acceptance_criteria`, hash IDs, "molecules" (epics).
- **beads core-concepts** (https://gastownhall.github.io/beads/core-concepts) — the formal beads model: lifecycle states, link types, ready queue, claims, semantic compaction.
- **Superset Conductor / `.conductor` / `.superset`** — referenced as the "small gitignored folder per project" ergonomics pattern. Inspired the `.docket/` shape. Conductor was *not* a name candidate — only an ergonomics reference.

## 3. Symphony patterns

Symphony writes skills as `<name>/SKILL.md` with:

- YAML frontmatter (`name`, `description`)
- Body sections: Goals / Inputs / Steps / Examples / Constraints
- Each skill is a single file in `.codex/skills/<name>/`

We considered this for `docket`'s prompts. **Dropped in favor of kanban style** (see § 4).

What we kept from symphony:

- The CLI-as-single-tool pattern (Symphony's `linear_graphql` is one tool with many GraphQL operations; `docket` is one CLI with many subcommands).
- The "policy lives in repo" idea (Symphony's `WORKFLOW.md`). Considered an analogous `.docket/workflow.md` per repo, eventually dropped — `config.toml` was rejected (hardcoded states are fine), and `workflow.md` collapsed into the prompts directory.

What we deliberately didn't draw:

- Elixir / Phoenix stack — too heavy for `docket`'s solo/small-team scope.
- Orchestration concepts (poller, isolated workspaces, agent runner) — `docket` is manually pushed, not observer-driven.
- The Symphony-style WORKFLOW.md prompt — not how we work on tasks. There's no observer in `docket`.

## 4. Kanban prompt-style findings

From the subagent that read kanban's prompt files:

**File structure:**

- Top-level: `AGENTS.md`, `CLAUDE.md` (no frontmatter)
- Workflows: `.clinerules/workflows/<name>.md`
- Planning: `.plan/docs/<name>.md`

**Frontmatter:** kanban does **not** use YAML frontmatter on its own prompts. Pure markdown with topic headers.

**Body structure recurring sections:**

- Numbered procedural steps ("1. Sync and gather context", "2. Collect commits")
- Before / During / After-style checklists ("Context loading checklist", "Before coding", "During coding", "After coding")
- Embedded shell/code blocks per step
- `@`-references to other files (e.g. `@AGENTS.md`)

**Adoption decision for `docket` prompts:**

- No YAML frontmatter
- Procedural body: numbered steps, embedded shell examples, checklists
- `@`-references between prompts (e.g. `commit.md` references `@tdd-pursuit.md` for the test-revision-cheat clause)

This is the style used in `templates/tdd-pursuit.md`, `create-task.md`, `commit.md`, `pr.md`.

## 5. TDD research distillation

The mattpocock TDD skill has six files. The agent that researched them distilled to one prompt by keeping load-bearing discipline and dropping anything not directly actionable inside a single TDD cycle.

**Kept:**

- RED-GREEN-REFACTOR loop
- Vertical-slice rule (one test → one impl → next test, never all tests then all impl)
- "Tests describe behavior through public interface" principle
- "Refactor only when green; revert if it goes red"
- "Mock only at true system boundaries; never your own modules"

**Dropped (and why):**

- Pre-test planning ("confirm with user what interface changes are needed") — `docket`'s task already arrived with `body` + `acceptance`; that *is* the plan.
- TypeScript-flavored code examples — would lock prompt to one ecosystem.
- Refactoring catalog (long-method, primitive-obsession etc.) — separate code-quality concern, not TDD pursuit.
- `interface-design.md` — design guidance for *before* you write the test; pursuit fires after.
- `deep-modules.md` — architectural advice, not execution discipline.

**Added that wasn't in the source** (the substantive `docket` contribution):

- **Five named anti-cheating disciplines**: task-revision cheat, test-revision cheat, tautology cheat, implementation-leak cheat, horizontal-slice cheat. Source assumed cooperative human; `docket` hardens against an agent that will rationalize its way out of a failing test.
- **Explicit asymmetry between modifying test and modifying implementation**, with two narrow exceptions for test edits (mechanical defects; refactor-phase structural changes that preserve assertions).
- **Escalation criteria**: legitimate exits from the loop (acceptance ambiguous, contradictory, test seems wrong, scope explosion, ≥3 attempts on same failure). Distinguishes "stuck" from "cheating."

These three additions are the load-bearing parts of `templates/tdd-pursuit.md`.

## 6. Beads contrast

From the subagent that surveyed beads (May 2026):

**Snapshot:**

- 23,411 stars, v1.0.3 (Apr 2026), pushed daily
- Go binary, ships via `brew`, `npm`, `curl | sh`
- Storage: **Dolt** (SQL/git hybrid), per-repo `.beads/`
- Schema includes: `id, title, status, priority, issue_type, description, owner, labels, dependencies[], parent, comments[], acceptance_criteria`
- Hash-based IDs (`bd-a3f8`) with hierarchical children (`bd-a3f8.1.1`)
- Five link types: `blocks`, `relates_to`, `duplicates`, `supersedes`, `replies_to`
- Has "molecules" (epics with children, parallel-by-default execution)
- Ships hooks/AGENTS.md via `bd setup claude` etc.
- No prose prompt templates shipped
- No TDD enforcement; `docs/TESTING_PHILOSOPHY.md` governs beads's *own* tests
- Anti-cheating discipline is about *push integrity*, not test-vs-implementation asymmetry

**Where `docket` would be reinventing beads:**

- Schema (acceptance, deps, status, priority) — beads has it
- Per-repo init + agent-CLI ergonomics — beads has it
- "Epic with children" grouping — molecules cover it
- JSON output, stealth/local mode — all there

**Where `docket` is genuinely different:**

1. TDD-pursuit prompt with named anti-cheating disciplines — beads has zero opinion here
2. Group = sequential single-branch + one-PR-at-end. Beads molecules are *parallel-by-default* and have no `branch_name` field.
3. Fresh-Claude-context-per-task spawn loop (planned: tmux + Claude session) — beads doesn't drive sessions.
4. Shipped prompt templates as editable filesystem artifacts — beads ships hooks, not authored prompts.
5. `[T-N]` commit tag as the *only* git bridge + deliberate refusal to track files / PR URLs in the DB.

**Three paths considered:**

- A: Adopt beads as the store, build `docket` as a TDD harness on top
- B: Build `docket` fully, accepting the duplication
- C: Pivot `docket`'s identity to "TDD execution harness; storage is incidental"

**Decision: B**, with explicit borrowing from beads where it adds value (see § 7). Justified because the load-bearing parts (TDD discipline, sequential group execution, prompt-pack ergonomics) are not in beads and are not trivially bolt-on.

> A follow-up question — "should docket use Dolt as the store, like beads does?" — was raised post-v1 and is captured separately in [`RESEARCH-STORAGE.md`](./RESEARCH-STORAGE.md). Decision: stay on bundled SQLite. Don't relitigate without a concrete trigger from § 8 of that doc.

## 7. Beads core-concepts adoption decisions

Concept-by-concept ADOPT / SKIP / ADAPT verdicts after reading the core-concepts page:

| Concept | Verdict | Reasoning |
|---|---|---|
| Status states (`open / in_progress / done`, `blocked` derived) | ADAPT | Beads' three real states are clean. `blocked` computed from unmet deps, never stored. |
| Hash IDs `bd-a3f8` + hierarchical `.1.1` | SKIP | Designed for distributed multi-agent merging across branches. `docket` is solo/small-team, single SQLite file. Flat `T-N` is fine. |
| Link types (`blocks`, `relates_to`, `duplicates`, `supersedes`, `replies_to`) | ADAPT to one | Only `blocks` drives `ready`. Others are nice-to-have metadata nobody queries. `deps` column captures `blocks`. |
| Ready queue (`bd ready`) | ADOPT | Single most valuable beads idea for an agent harness. ~10-line filter on existing data. Without it, an agent has to read full task list and reason about deps. |
| Atomic claims | SKIP | Beads needs this because it assumes concurrent agents. `docket` is sequential within a group by design. `status='in_progress'` flip under a SQLite transaction is enough. |
| Acceptance criteria | ADOPT | TDD harness's anchor. Already in our schema. |
| Comments / activity log | SKIP | Git log + `[T-N]` tags *is* the activity log. No second one. |
| Semantic compaction | SKIP | Only matters under context-window pressure. Defer with a `summary` column lazily populated on `docket close` — don't pre-build. |
| Priority (0-4) | ADOPT | Single column, default 2. |
| Labels | SKIP | `group_id` partitions. Labels become folksonomy nobody curates. |
| Audit trail (separate table) | SKIP | `created_at`/`updated_at` cover 95%. Git is the rest. |
| Owner / assignee | SKIP | Solo/small-team. The agent claiming `in_progress` is the assignee. |
| Hierarchy (parent/molecules/epics) | SKIP | Our `groups` already gives one level. Beads' molecules/formulas are an entire workflow engine — out of scope. |
| `--json` output | ADOPT | Cheap (serde). The whole point of `docket` is being agent-shaped. |
| AGENTS.md generation on init | ADOPT (deferred) | Cheapest way to make `docket` self-documenting in any repo. Listed in ROADMAP. |
| Formulas / molecules / cooking / gates / wisps / swarms | SKIP all | This is the philosophical break — see § 9. |

## 8. Schema iteration history

The schema went through several rounds. Captured here so future edits don't repeat the same arguments.

**v0 (initial proposal, pre-truncation):**

- `tasks(id, title, body, status, priority, due_date, assignee, ...)` — generic kanban shape
- Rejected as PM-flavored, not agent-shaped.

**v1 (richer agent-shape):**

- `tasks` + `archive` + `groups`
- `tasks(id, title, body, scope, acceptance, deps, status, session_log, ...)`
- Considered a `task_sessions` link table for archaeology with shape: `(id, task_id, claude_session_id, started_at, ended_at, outcome, commits_touched, summary)` — link from task → Claude transcript, commits made, one-line summary. Dropped: most tasks finish in one session and archaeology is better served by `git log --grep='\[T-N\]'`.

**v2 (after grilling):**

- Dropped `task_sessions` table — most tasks finish in one session, archaeology can use git.
- Dropped `commits_touched` and `pr_url` — git already holds these; `[T-N]` commit tag is the bridge.
- Dropped `scope` — every folder has a `CLAUDE.md`; that's the scope layer.
- Dropped `archive` table — `status='done'` is enough; if queries get slow later, archive then.
- Dropped `session_log` text column — ambiguous purpose, not load-bearing.
- Dropped `config.toml` for board states — hardcoded states are fine.
- Dropped `workflow.md` per repo — collapses into the prompts directory.
- Considered `completion_notes` column — specified as the agent's only post-done obligation (one line written when marking task done, distinct from a running session log). Deferred to v0.3 (parked in ROADMAP § Open questions).

**v3 (final, after beads borrowing):**

- `tasks(id, title, body, acceptance, deps, status, priority, group_id, created_at, updated_at)` — added `priority`
- `groups(id, name UNIQUE, branch_name, description, state, created_at)`
- `group_id` is nullable — tasks can be standalone, work on the current branch without group ceremony
- States: `open / in_progress / done` (beads naming), `blocked` is computed

## 9. Deliberate rejections

Things we decided not to build, with reasoning so they don't accidentally re-enter scope.

- **Workflow engine.** No formulas, no `docket cook`, no `docket swarm`. The moment we add a TOML formula format we've stopped being a SQLite task list with a TDD harness. Orchestration goes in the shell that calls `docket ready`.
- **Atomic claims.** Sequential execution by design. No concurrent agents to coordinate against.
- **Comments table.** Git is the activity log.
- **Multiple link types.** Only `blocks` drives behavior.
- **Hierarchy beyond groups.** Groups are one level. Beads-style molecules-with-children would lure us toward parallelism we explicitly don't want.
- **Labels.** Folksonomy without an authority becomes noise.
- **Owner / assignee.** Multi-user is v0.3+ and would imply structural changes (audit, lock semantics) we don't want now.
- **Markdown task file.** Considered (`TASKS.md`, then a folder of files per task). Rejected — appending to a flat file forces full reads, no ID lookup, no queries.
- **Per-task prompt file.** Considered. Replaced by per-prompt-type files (one `tdd-pursuit.md`, not one per task).
- **MCP integration.** Pollutes the context window with schema on every call, and the per-call overhead compounds. User's framing: *"it's polluting the context a lot and after every issue creation I don't want to have the MCP call."* Plain CLI is lighter — one shell-out, no resident schema.
- **Per-task PRs.** Three or four tasks usually combine into one branch and one PR, not one PR per task. User's framing: *"a task probably will not even have a PR. Three or four tasks combined together in a branch probably might have a PR... I don't want every individual task to create a PR that just bloats the entire repository."* This is why `docket` ties PRs to groups, not tasks.
- **TUI.** Mentioned during design (Textual / Rust ratatui). Deferred — `--json` covers most cases.
- **`workflow.md` per repo.** Symphony pattern. Dropped — collapses into the embedded prompts.
- **`config.toml` per repo.** Considered for board states. Dropped — hardcoded states are fine.

## 10. Stack history

- **v0:** Python + click (CLI) + Textual (TUI). Rejected — `pipx` distribution + venv lifecycle is heavier than a single binary.
- **v1 (final):** Rust + clap + rusqlite (bundled). Single static binary, `cargo install` from source today, `curl | sh` later via GitHub Releases.

## 11. Naming history

The working name through v1 design was `tb` (two-letter placeholder). Settled on **`docket`** — legal-docket framing matches the "queue of items to work through" model, distinctive on crates.io, and reads naturally in a sentence ("on the docket").

Alternatives considered:

- `cue` — symphony-shaped, "agent's cue to act," 3 chars
- `beat` — symphony-shaped, "what's the next beat," 4 chars
- `pickup` — descriptive of the agent-pickup framing, 6 chars
- `hop` — agent-pickup feel without the music metaphor
- `slate`, `cairn`, `rung` — surfaced later; `slate` was the runner-up

Conductor was *not* a name candidate — Superset Conductor was cited only as a reference for the gitignored-folder ergonomics pattern.

## 12. Open questions

Parked in ROADMAP § "Open questions parked from design":

- Auto-close groups when all tasks done, or always manual `docket group close`?
- Status validation — should `docket status` reject unknown states?
- `completion_notes` column on tasks — needed for `tdd-pursuit` exit summaries?

Revisitable when usage shows whether they matter.

## 13. Subagents consulted

Four subagents were spawned during design, each with a focused brief:

1. **Kanban prompt patterns** — read `.codex/`, `.claude/`, `.cline/`, `.factory/` etc. in `/Users/nandakumar/Personal/kanban/`. Returned the procedural / no-frontmatter style used in `docket`'s prompts.
2. **TDD prompt distillation** — fetched mattpocock skills, distilled to ~150 lines, added the five anti-cheating disciplines.
3. **Beads contrast** — fetched beads README/docs, produced the comparison table and the three-paths recommendation.
4. **Beads core-concepts adoption** — read https://gastownhall.github.io/beads/core-concepts, produced the per-concept ADOPT/SKIP/ADAPT verdicts in § 7.

Their full reports live in the conversation transcript at `~/.claude/projects/-Users-nandakumar-Personal-symphony/`. Sections § 4–§ 7 above are distillations.

## 14. Methodology note on truncation

Mid-design, the session was transferred via `/desktop` to Claude Desktop, then resumed in CLI. The CLI process started fresh and lost the earlier portion of conversation context. Recovery was via grepping the on-disk JSONL transcript at `~/.claude/projects/-Users-nandakumar-Personal-symphony/<session-id>.jsonl`, which retained the full history.

Lesson: when a long design session is going somewhere, capture the convergent decisions in a file (this one) before any session handoff. The transcript JSONL is the durable record; chat context is not.
