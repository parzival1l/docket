# docket

Agent-shaped task tracker with a TDD execution harness, for solo and small-team coding work.

`docket` is a per-repo task store (single SQLite file, gitignored) plus a curated set of prompt templates that turn an agent's task pickup into a disciplined TDD loop. It complements git and GitHub — it does not replace them.

## What docket owns vs. what it delegates

**`docket` owns:**

- Structured task definition with first-class `acceptance` (testable criteria) and `deps` fields
- A `ready` queue: the next task whose deps are all `done`
- Groups: a sequential-execution unit (multiple tasks, one branch, one PR)
- Four authored prompts (`tdd-pursuit`, `create-task`, `commit`, `pr`) embedded in the binary

**`docket` delegates:**

- File-level history → git (`git log --grep='\[T-N\]'`)
- Code review, discussion, approval → GitHub PRs
- Project-level rollup → whatever PM tool you use

**The line `docket` refuses to cross:** workflow-engine territory. No formulas, no swarms, no DSLs. Orchestration belongs in the shell that calls `docket ready` in a loop, not in `docket`.

## Install

### Quick install (recommended)

```bash
curl -LsSf https://raw.githubusercontent.com/parzival1l/docket/main/install.sh | bash
```

Detects your platform, downloads the matching binary from the latest GitHub Release, verifies its SHA-256, and drops `docket` into `~/.local/bin`. Re-run any time to update.

Make sure `~/.local/bin` is on your `$PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

To pin a specific version: `bash -s -- --version v0.0.1`. To install elsewhere: `DOCKET_INSTALL_DIR=/somewhere/bin bash`.

### From source

```bash
git clone https://github.com/parzival1l/docket.git
cd docket
cargo install --path .          # → ~/.cargo/bin/docket
```

Single static binary. Tier-1 targets: macOS (x86_64, aarch64) and Linux (x86_64, aarch64).

### Plugin (Claude Code)

`docket` ships with a Claude Code plugin that exposes the CLI verbs as slash commands inside an active session — `/docket:create-task`, `/docket:start T-N`. The plugin and the CLI are independent installs; the plugin shells out to the `docket` binary on your `$PATH`.

Install from inside Claude Code:

```
/plugin marketplace add parzival1l/docket
/plugin install docket@docket
```

That registers the marketplace and enables the plugin in `~/.claude/settings.json`. Subsequent native sessions pick it up automatically. Update with `/plugin marketplace update docket`; pick up edits inside an active session with `/plugin reload`.

See [`plugin/CONVENTIONS.md`](./plugin/CONVENTIONS.md) for the pattern when adding new commands.

### Cutting a release (maintainer)

Releases are tag-driven, but tagging itself is automated. You drive a single human signal — *"the next version is X"* — and the rest is mechanical.

One-time setup:

```bash
cargo install cargo-release
```

The flow:

```bash
# 1. Start a release branch off main
git checkout main && git pull
git checkout -b release/0.0.2

# 2. cargo-release bumps Cargo.toml + Cargo.lock, splits CHANGELOG.md
#    [Unreleased] into a new [0.0.2] - YYYY-MM-DD section, and commits as
#    `release: 0.0.2`. It does NOT push or tag (see release.toml).
cargo release 0.0.2 --execute

# 3. Edit the new ## [0.0.2] section in CHANGELOG.md to fill in entries,
#    then `git commit --amend` (or add a follow-up commit on the branch).

# 4. Push the branch and open a PR
git push -u origin release/0.0.2
gh pr create --title "release: 0.0.2"
```

Merging the PR fires `.github/workflows/auto-tag.yml`, which reads the version from `Cargo.toml`, creates and pushes `v0.0.2`, then triggers `release.yml`. That cross-builds all four targets, generates checksums, and publishes a GitHub Release with notes pulled from the matching `## [0.0.2]` section of [`CHANGELOG.md`](./docs/CHANGELOG.md).

To re-release an existing tag (e.g. after fixing the build pipeline): `gh workflow run release.yml -f tag=v0.0.2`.

See [`CHANGELOG.md`](./docs/CHANGELOG.md) for version history and [`release.toml`](./release.toml) for the cargo-release config.

## Quick start

```bash
cd /path/to/your/repo
docket init                              # creates .docket/db.sqlite, appends .docket/ to .gitignore

docket add "uploader retries failed parts" \
  --acceptance "retries each part up to 3x; succeeds if any part succeeds" \
  --group "uploader-fixes"

docket add "uploader emits per-part metrics" \
  --acceptance "metric uploader_part_total has labels {status, attempt}" \
  --deps T-1 \
  --group "uploader-fixes"

docket ready                              # T-1 only — T-2 is blocked by T-1
docket done T-1                           # T-2 now ready
docket ready                              # T-2 surfaces
```

## CLI

| Verb | Purpose |
|------|---------|
| `docket init` | Create `.docket/db.sqlite` in current repo, append `.docket/` to `.gitignore` |
| `docket add <title>` | Create a task; flags: `--body --acceptance --deps --priority --group` |
| `docket ls` | List tasks; flags: `--status --group --json` |
| `docket show T-N` | Full task with body, acceptance, deps with resolved states |
| `docket ready` | Tasks with `status=open` and all deps `done` |
| `docket blocked` | Inverse of ready: tasks with unmet deps (debug view) |
| `docket status T-N <state>` | Set status (`open`, `in_progress`, `done`, or any string) |
| `docket done T-N` | Convenience for `docket status T-N done` |
| `docket rm T-N` | Delete a task |
| `docket prompt <name>` | Print an embedded prompt (`tdd-pursuit`, `create-task`, `commit`, `pr`) |
| `docket group new <name>` | Create a group; flags: `--branch --description` |
| `docket group ls` | List groups with `done/total` task counts |
| `docket group show <name>` | Group detail + tasks |
| `docket group close <name>` | Mark group closed |

Every list/show command supports `--json` for agent consumption.

## Schema

Two tables in `.docket/db.sqlite`:

```
tasks(id, title, body, acceptance, deps, status, priority, group_id, created_at, updated_at)
groups(id, name UNIQUE, branch_name, description, state, created_at)
```

- `id` is integer-incrementing; rendered as `T-N`.
- `deps` is a JSON array of integer task IDs (e.g. `[3, 5]`); the only link type is `blocks`.
- `status` is `open | in_progress | done` by convention. `blocked` is *computed* from unmet deps, never stored.
- `priority` is 0..4, default 2 (lower = more urgent, beads convention).
- `group_id` is nullable — tasks don't have to be in a group.

## Prompts

Four authored prompts ship embedded in the binary, accessed via `docket prompt <name>`:

- **`tdd-pursuit`** — the RED-GREEN-REFACTOR execution loop with five named anti-cheating disciplines (task-revision, test-revision, tautology, implementation-leak, horizontal-slice cheats), explicit asymmetry between modifying test vs. implementation, and escalation criteria for legitimate exits.
- **`create-task`** — turn a one-line ask into a structured task with testable acceptance criteria.
- **`commit`** — every commit during a task carries `[T-N]` so git history cross-links to `docket`.
- **`pr`** — one PR per group at completion, summary aggregated from task bodies and acceptances.

Sources at [`templates/`](./templates/). They're embedded into the binary via `include_str!` at build time — edit and rebuild to change them.

## Cross-system link

The single bridge between `docket` and git/GitHub is the **`[T-N]`** tag in commit messages. To find what changed for a task:

```bash
git log --grep='\[T-3\]'
```

`docket` does not track files-touched, commit SHAs, or PR URLs — git already does.

## Design choices (load-bearing)

- **Per-repo, gitignored `.docket/`.** Per-developer state, no binary merge conflicts. Like `.conductor/`, `.superset/`.
- **Two tables, no archive, no audit, no comments, no labels.** Done tasks sit in `tasks` with `status=done`. Git is the activity log.
- **Sequential execution, not parallel.** A group runs tasks one at a time on one branch; no atomic claims because there's nothing to coordinate against.
- **One PR per group, not per task.** Three or four tasks usually batch into one branch and one PR. Per-task PRs would bloat the repo with review noise.
- **`acceptance` as first-class field.** This is the contract `tdd-pursuit` enforces.
- **Embedded prompts.** Source-of-truth is in the binary; edit `templates/` and rebuild to change them.

## Roadmap

See [ROADMAP.md](./docs/ROADMAP.md) for the deferred items.
