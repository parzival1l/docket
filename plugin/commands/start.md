---
description: Begin work on a docket task inside the current Claude Code session — assembles the task's body, acceptance, and the tdd-pursuit harness as your operating brief.
argument-hint: "T-N"
allowed-tools: [Bash]
---

# /docket:start

In-session twin of the CLI `docket start <id>`. The CLI version of `docket start` assembles a task's brief and (in later phases) spawns a fresh Claude session in a new tmux window. This command is the equivalent for when you are *already* inside a Claude Code session: it fetches the same brief and continues the current conversation against it. No tmux, no spawn — same content.

## Process

1. Run `docket start $ARGUMENTS`. The CLI loads the task, marks it `in_progress`, and prints the assembled prompt (task title, body, acceptance criteria, plus the embedded `tdd-pursuit` execution discipline) to stdout.

2. **Treat that stdout as your operating brief for the rest of this conversation.** Follow the `tdd-pursuit` discipline strictly — RED → GREEN → REFACTOR, the five named anti-cheating rules, the explicit asymmetry between modifying tests vs. implementation, and the escalation criteria for legitimate exits.

3. Acknowledge in one line: `Starting T-N: <title>` — nothing more. Do not summarize the brief back at the user; they wrote it.

4. Begin work on the task immediately. From this point forward you ARE the coding agent for this task; this is not a meta-discussion.

## Failure modes

- **Task not found / `docket` not on PATH.** Surface the CLI error verbatim and stop. Do not try to invent a task brief.
- **Task is `done`.** The CLI refuses to restart `done` tasks. Surface that and ask the user whether they want to reopen it (`docket status $ARGUMENTS open`) before re-running.
- **No argument provided.** Suggest `docket ready` to see what's next, then ask which ID to start.

## Why this exists separately from the CLI

The CLI `docket start <id>` has multiple delivery modes — Phase 1 prints to stdout (composes with `| claude`, `| pbcopy`, `> /tmp/p.md`), later phases spawn a fresh Claude in tmux/iterm/zellij. Those modes assume you're starting *outside* a session. This command assumes you're already *inside* one — so it skips the spawn and just routes the brief into the current conversation.
