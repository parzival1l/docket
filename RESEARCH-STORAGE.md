# Research Notes — Storage choice (docket vs Dolt / beads)

Companion to `RESEARCH.md`. Captures the Dolt-as-alternative-store question that came up after the v1 design pass, and why it was rejected. Saved here so the answer doesn't have to be re-derived from scratch the next time the question surfaces.

## 1. The question

> "Would [Dolt](https://github.com/dolthub/dolt) be a good fit instead of SQLite? I'm mainly comparing this app to the beads project, which uses Dolt server mode."

Beads core-concepts on Dolt server mode: https://gastownhall.github.io/beads/core-concepts#dolt-server-mode

## 2. What Dolt is

Dolt is "Git for data" — a SQL database (MySQL-compatible wire protocol) with versioned table contents. You can `commit`, `branch`, `merge`, and `diff` rows the same way Git does for files. Ships as a Go binary; runs embedded or as a server.

## 3. Why beads chose Dolt

Read beads's core-concepts page through one lens: **parallel multi-agent coordination**. Hash IDs, atomic claims, semantic compaction, server mode, link types, molecules-with-children — these are all consequences of "many agents touching the same board, possibly across machines, possibly across teams."

Dolt is the natural storage for that problem. It gives beads:

- Versioned data so concurrent agents don't clobber each other
- Server mode so a team can share one source of truth
- An audit trail without building one
- Branching at the *data* layer for experimental edits

## 4. Why this is the wrong shape for docket

Docket's design (see `RESEARCH.md` §§ 6–9) explicitly opted out of every problem Dolt solves:

| Capability Dolt provides | docket's stated position |
|---|---|
| Versioned rows, merge across branches | Sequential by design — one agent on one branch at a time |
| Server mode for shared team board | `.docket/` is **per-repo and gitignored**; per-developer state is a feature |
| Audit trail of row changes | "Git is the activity log" — `[T-N]` commit tag is the only bridge |
| Atomic claims for concurrent agents | Skipped in § 7 — `status='in_progress'` flip in a SQLite transaction is enough |
| Time-travel queries | `git log --grep='\[T-N\]'` covers the archaeology |
| Cross-machine DB sync | Explicit non-goal; v0.3 plans JSONL export/import as the wire format |

Adopting Dolt would re-introduce most of the complexity § 7 deliberately rejected.

## 5. What it would cost docket concretely

1. **Distribution story collapses.** Current: single Rust binary, `~3 MB`, `cargo install --path .`, no runtime deps. Dolt has no first-class embedded Rust crate — you'd ship a Rust CLI that also requires a Dolt process and an async MySQL client. The "single binary in `.docket/`" ergonomic is load-bearing for the solo-developer use case.
2. **Re-pays design decisions already won.** The v3 schema (§ 8) and the per-concept ADOPT/SKIP table (§ 7) were the convergent end of a long design pass. Switching stores reopens all of them.
3. **Operational surface area.** Even embedded, Dolt is a database engine with branches and merges. SQLite is a file. For a tool meant to sit next to a checkout, the file wins.
4. **Pulls toward beads's shape.** The moment you have versioned data and server mode, the temptation to add atomic claims, comments, multi-agent molecules, etc. compounds. Each is small individually; together they're beads. § 9 ("the line docket refuses to cross") becomes harder to hold.

## 6. Where Dolt could legitimately re-enter scope

One place, deferred to v0.3+:

- **Optional shared team board.** ROADMAP lists "Optional Postgres sync — for shared team boards" with JSONL as the wire format. If that work is ever queued and the team wants a *queryable* shared board (not just import/export), Dolt is a credible alternative to Postgres because "boards as branches" maps cleanly onto it.

The decision posture: **defer until that v0.3 work is real**. By then there will be evidence about whether teams actually want a shared queryable board, and the choice (Dolt / Postgres / "JSONL is enough") can be made with data instead of speculation.

## 7. Decision

**Stay on bundled SQLite via `rusqlite`.** Single binary, no runtime deps, file-shaped storage that lives next to the checkout. This is the shape that lets docket stay small, which is the whole point.

## 8. Triggers that would force a revisit

- Real demand for **multiple agents working the same board concurrently** (not "an agent at a time on one branch")
- Real demand for **cross-machine / cross-team shared boards** beyond what JSONL export/import covers
- The v0.3 "team sync" feature actually getting queued, with a queryable-shared-board requirement
- A first-class embedded Dolt crate for Rust appearing (would lower the distribution-story cost meaningfully)

Until any of these are concretely true, this question is settled. Don't relitigate.
