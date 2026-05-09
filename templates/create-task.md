# Create Task

Turn a one-line ask into a structured `docket` task with testable acceptance criteria.

## Inputs

- A short description of work from the user (the *ask*)
- Current repo context

## Output

A single `docket add` invocation with `title`, `body`, `acceptance`, optional `deps`, optional `group`, optional `priority`.

## Steps

1. **Restate the ask in one sentence.** Strip out solution-shape words. The `title` describes the *outcome*, not the implementation.
   - Bad: "refactor uploader to use queue"
   - Good: "uploader retries failed parts without dropping them"

2. **Write a body** (1-3 sentences) covering:
   - The current behavior or symptom
   - The intended behavior
   - One sentence on why now (the trigger)

3. **Write acceptance criteria.** This is the contract `@tdd-pursuit.md` will enforce. Each criterion must be:
   - **Testable** — there is a test you can sketch in your head against it. If you can't, the criterion is too vague.
   - **Behavioral, not internal** — describes what the system *does*, not how it's structured.
   - **Specific** — values, conditions, edge cases named. "Handles errors gracefully" is not acceptance; "returns `Err(NotFound)` when key is missing" is.

   If you cannot write at least one testable criterion, **stop and escalate**. The task isn't ready.

4. **Note dependencies.** If this task is genuinely *blocked* by another existing task, list its ID. Do not list "related" tasks — only blockers. Most tasks have no deps.

5. **Group (optional).** If the user said this is part of a batch ("the auth-rewrite work"), assign the group name. Otherwise leave ungrouped — ungrouped tasks work fine on the current branch without group ceremony.

6. **Priority (optional).** Default is `2`. Use `--priority=1` only for genuinely urgent work; `--priority=3` for low. Don't fiddle.

7. **Run `docket add`.**

   ```bash
   docket add "<title>" \
     --body "<body>" \
     --acceptance "<criterion 1>; <criterion 2>; ..." \
     --deps "T-3,T-5" \
     --group "auth-rewrite" \
     --priority 2
   ```

## Acceptance smell-test

Before running `docket add`, ask:

- [ ] Can I name the test that proves criterion 1 passes? (If no, rewrite it.)
- [ ] Does any criterion describe an *implementation choice* rather than a behavior? (If yes, rewrite it.)
- [ ] Does the title contain "refactor", "use", "switch to", "implement"? Often a smell — title is describing means, not ends.
- [ ] Could I hand this task to a fresh Claude session with zero prior context and have them know when they're done? (If no, body or acceptance is missing context.)

## When to escalate instead of creating

- The ask is exploratory ("look into why X is slow") — that's research, not a task. Suggest doing the research in-session, then creating a *concrete* task from the findings.
- The ask is a question, not work.
- The ask conflates several pieces of work — break it into multiple tasks (or a group) before adding.
