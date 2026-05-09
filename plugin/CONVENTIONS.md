# Plugin conventions

How to add a new command to the `docket` plugin without re-deriving the shape every time. Lives at the plugin root so it stays out of the user-facing slash menu (only `commands/*.md` is auto-discovered).

For the authoritative schema, read the Claude Code plugin docs once:

- Plugins overview & layout: <https://code.claude.com/docs/en/plugins.md>
- Frontmatter reference (commands, skills): <https://code.claude.com/docs/en/plugins-reference.md>

Everything below is the *house style* on top of that.

## File layout

```
plugin/
├── .claude-plugin/plugin.json   # manifest — `name: docket` becomes the namespace
├── commands/<name>.md           # each file = one slash command, /docket:<name>
└── CONVENTIONS.md               # this file (root, not commands/)
```

One file per command. Filename (kebab-case) is the command name after `/docket:`. Don't nest `commands/` directories — flat layout, no exceptions.

Slash menu order is alphabetical by filename. If you need a specific ordering, name accordingly; don't introduce numeric prefixes (they'd show up in the command name).

## Frontmatter

In this order, with these fields:

```yaml
---
description: One sentence. What the command does and when to reach for it. Shown in the slash menu — terse, verbs first.
argument-hint: "T-N"                # optional. shown in autocomplete; mirror the CLI argument shape
allowed-tools: [Bash, AskUserQuestion]  # optional but preferred. restrict to what the command actually needs
---
```

- **description** is required. Treat it as marketing copy for a 1-line slot.
- **argument-hint** mirrors the CLI when there's a CLI twin (e.g. `T-N` for `/docket:start`).
- **allowed-tools** narrows blast radius. If the command only shells out, list `[Bash]`. Add `AskUserQuestion` only when you genuinely quiz the user. Don't add `Write`/`Edit` to a planning command.

## Body shape

Six sections in this order. Skip a section only if it genuinely doesn't apply.

### 1. Title + one-paragraph identity

```
# /docket:<name>

<One paragraph. What it does. Relationship to the CLI verb if there is one (twin / shim / wrapper).>
```

State explicitly whether the command **is** a coding agent (`/docket:start`) or **is not** (`/docket:create-task`). That one line shapes the rest of the body — a planning command shouldn't quietly start editing files, and a coding-agent command shouldn't waste turns asking permission.

### 2. Process

Numbered steps. Each step ends with a concrete action: *run* X, *ask* Y, *write* Z. Avoid passive voice. Avoid "consider"; either it's a step or it isn't.

If the command has a "default vs. capability" axis (e.g. create-task defaults to one task, splits on request), name the default in the first step and the trigger condition for the capability.

### 3. Embedded rule blocks (optional)

Use XML-tagged blocks for reusable rule sets the model should treat as a literal contract:

```
<vertical-slice-rules>
- ...
</vertical-slice-rules>
```

Keep them small. If a block is growing past ~10 lines, it probably belongs in `templates/<name>.md` at the docket repo root and should be fetched at runtime via `docket prompt <name>` — single source of truth, edit-and-rebuild.

### 4. Failure modes

List the realistic ways the underlying CLI fails and how to surface each. Look at `/docket:start`'s section for the shape: task-not-found, already-done, missing argument. Always:

- Surface the CLI's stderr verbatim. Don't paraphrase; the user knows their CLI.
- Stop on hard failures. Don't fabricate a fallback brief.
- For ambiguous states (e.g. "task already done"), propose the next CLI invocation rather than auto-fixing.

### 5. Receipt-only output

When the command finishes, print exactly what the user needs to act on. Examples:

- create-task: `T-3 created — \`docket show T-3\``
- start: `Starting T-N: <title>` (one line, then begin work)

Do NOT summarize what was done. Do NOT restate the brief. The CLI mutation + the receipt line ARE the output.

### 6. Anti-patterns (optional)

If the command has specific failure modes that aren't behavioral CLI errors but model-behavior smells (e.g. "splitting one ask into multiple tasks", "inlining file paths in the body"), list them at the bottom. Three to six bullets, no more.

## Default vs. capability

Many docket commands have a *default behavior* and an *optional capability*. Make the default trivially cheap (single `docket add`, no questions) and gate the capability behind an explicit signal (a phrase from the user, a CLI flag, an obvious necessity from the discussion).

Bias: when in doubt, do the smaller thing and let the user ask for the bigger thing. The cost of asking again is one turn; the cost of an unasked-for split is correcting state.

## When to reach for `AskUserQuestion`

- A required field is missing and unguessable from context (e.g. a testable acceptance criterion).
- You're about to do something the user didn't explicitly authorize and can't easily undo (creating multiple tasks when they asked for one; mutating state across more than one record).
- A dependency/order matters and the discussion didn't resolve it.

Don't ask to confirm the obvious. Don't ask before single-record reads or trivially reversible writes.

## When NOT to write a new command

If the work fits cleanly inside an existing CLI verb, just document the verb in `README.md` and skip the plugin command. The plugin earns its keep by composing CLI verbs with model behavior (multi-step asks, in-session brief assembly, conversational quizzing). A 1:1 alias of a single CLI verb is dead weight.

## Cutting a release

The plugin uses **pinned versioning**, not auto-SHA. Installed users only see a new build when the version string bumps — pushing a commit without a bump does nothing for them. This is intentional: it lets us iterate on `main` without dripping half-finished commits to anyone running `/plugin marketplace update docket`.

The plugin's version tracks the CLI's `Cargo.toml` version one-to-one. When you cut a CLI release:

1. Bump `version` in `Cargo.toml` and `Cargo.lock` (existing flow).
2. Bump `version` in **both** `plugin/.claude-plugin/plugin.json` AND the plugin entry in `.claude-plugin/marketplace.json`. They must match.
3. Add a CHANGELOG entry under `CHANGELOG.md`. If the change is plugin-only, prefix the bullet with `plugin:` so the diff is greppable.
4. Commit, tag (`v0.0.2`), push tag — the existing `release.yml` workflow handles the CLI binary release.

The plugin doesn't need its own tag — `/plugin marketplace update docket` re-fetches `main` and reads the bumped version from the manifests. The CLI tag is the single source of truth.

Revisit this rule only if the plugin starts shipping changes independently of the CLI (e.g. a slash-command-only fix). If that happens repeatedly, decouple the version fields and start a separate `plugin-vX.Y.Z` tag series.
