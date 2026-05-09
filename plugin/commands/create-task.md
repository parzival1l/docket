---
description: Convert the in-progress discussion into one or more docket tasks via `docket add`. Defaults to one task per ask; splits only on explicit request.
argument-hint: "[optional extra context]"
allowed-tools: [Bash, Read, AskUserQuestion]
---

# /docket:create-task

Turn the discussion that just happened into structured `docket` tasks with testable acceptance criteria.

You are not a coding agent in this command. Do not edit files, do not implement the work. Your one job is to take the conversation that just happened (plus any extra context in `$ARGUMENTS`) and convert it into the right number of `docket add` invocations.

**Default to one task per ask.** If the user asks for "a task for X", create one task for X — do not silently split it into a multi-slice plan. Splitting is a capability you reach for only when (a) the user explicitly asks ("break this down", "split into tasks", "make N tasks"), or (b) the discussion clearly contains multiple independent outcomes that cannot fit one testable acceptance set without becoming vague. When in doubt, create one and let the user ask for a split.

## Process

### 1. Restate the outcome

In one sentence, restate the *outcome* the discussion is converging toward. Strip solution-shape words ("refactor", "use X", "switch to") — outcomes describe behavior, not implementation.

If the discussion is exploratory ("look into why X is slow"), stop and escalate (see *When to escalate*). Don't create a task.

### 2. One task or many?

Default: **one task**. Only split when one of these is true:

- The user explicitly asked to split ("break this down", "split into tasks", "make N tasks").
- A single acceptance set would have to mix unrelated behaviors to cover the ask, forcing you to write vague criteria like "and also handles Y".

If you do split, apply the rules below. If you don't, treat steps 3+ as operating on a single slice.

<vertical-slice-rules>
- Each slice delivers a narrow but COMPLETE behavior change, end-to-end.
- A completed slice is verifiable on its own — its acceptance criteria are testable in isolation.
- Slices may have `--deps` between them; capture those.
- Do NOT split by layer (one task for "schema", one for "API", one for "UI"). That is horizontal slicing and produces tasks that aren't independently verifiable.
</vertical-slice-rules>

### 3. Draft each slice

For every slice, draft the four fields below. The per-task contract — naming, acceptance smell-tests, escalation rules — lives in the embedded `create-task` prompt. Fetch it once with `docket prompt create-task` and apply it per slice; do not duplicate its rules here.

<task-template>
**Title:** describes the outcome, not the implementation. No "refactor", "use X", "switch to", "implement".

**Body** (1–3 sentences): current behavior or symptom; intended behavior; one sentence on why now.

**Acceptance:** semicolon-separated list of testable criteria. Each criterion must be
  - **testable** — you can name the test that would prove it
  - **behavioral, not internal** — what the system does, not how it is structured
  - **specific** — values, conditions, edge cases named

**Deps** (optional): only tasks that *block* this one. Not "related" tasks.

**Group** (optional): if the user framed the work as a batch ("the auth-rewrite work"), assign that group name. Otherwise leave ungrouped.
</task-template>

Do NOT inline file paths, function names, or code snippets in the body — they go stale fast. The body describes behavior; the agent picking up the task will read the code itself.

### 4. Quiz the user only when needed

Don't confirm the obvious. Only stop and ask via `AskUserQuestion` when:

- You couldn't write at least one testable acceptance criterion. Quote what's vague back at the user and ask for the concrete behavior.
- You decided to split into multiple slices. Show the numbered list (title + acceptance per slice) and confirm the breakdown before writing.
- A dep relationship is ambiguous and matters for ordering.

For a single task with clear acceptance, skip the quiz and go straight to step 5. **Do NOT call `docket add` while you still have a question outstanding.**

### 5. Publish

For each approved slice, in dependency order (deps before dependents), run:

```bash
docket add "<title>" \
  --body "<body>" \
  --acceptance "<criterion 1>; <criterion 2>" \
  --deps "T-X,T-Y" \
  --group "<group>" \
  --priority 2
```

Defaults: omit `--priority` (uses 2). Omit `--group` if ungrouped. Omit `--deps` if no blockers.

Capture the printed `T-N` for each task. After all slices are created, print one line per task and nothing else:

```
T-3 created — `docket show T-3`
T-4 created — `docket show T-4` (blocked by T-3)
```

Don't summarize the work, don't restate what was created — the IDs are the receipt.

## When to escalate instead of creating

Three asks are not tasks. Say so explicitly and propose the next step rather than calling `docket add`:

- **Exploratory.** "Look into why X is slow." That's research. Suggest doing it in-session and creating a concrete task from the findings.
- **Question, not work.** Answer it; don't track it.
- **No testable criterion possible.** If you can't sketch one test for the slice, the spec is too vague to track. Quiz the user for the missing concrete behavior; if they can't supply it either, escalate.

## Anti-patterns

- Splitting a single ask into multiple tasks when the user asked for one. Default is one task; split only on explicit request or genuine necessity.
- Creating one task per layer (schema task, API task, UI task) — see vertical-slice-rules.
- Title that names the means ("refactor uploader to use queue") instead of the end ("uploader retries failed parts without dropping them").
- Acceptance like "handles errors gracefully" or "is performant" — not testable; rewrite or quiz.
- Calling `docket add` while a question is still outstanding.
- Inlining file paths, function names, or code snippets in `--body`.
