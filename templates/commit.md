# Commit (within a task)

You are committing while working on `docket` task **T-N**. The commit message must link back to the task and capture the *why*.

## Format

```
<type>(<scope>): <subject>  [T-N]

<body — 1-3 sentences on rationale, sourced from the task's body>

<optional footer>
```

## Steps

1. **Identify the task.** You know which T-N you're on (from the spawn prompt or `docket show T-N`). Embed `[T-N]` in the subject line.

2. **Choose `type`.** Conventional commits:
   - `feat` — new behavior visible to users
   - `fix` — bug fix
   - `refactor` — internal restructure, no behavior change
   - `test` — only tests changed (during RED, this is your first commit)
   - `docs` — documentation only
   - `chore` — tooling, deps, build

3. **Choose `scope`.** The module or area touched. Single word, lowercase. Optional but preferred.

4. **Write the subject.**
   - Imperative mood: "add", "fix", "rename" — not "added", "adds".
   - ≤72 chars *including* the `[T-N]` tag.
   - No trailing period.

5. **Write the body.**
   - Pull rationale from the task's `body` (`docket show T-N`). Don't paraphrase if the task body says it tightly already.
   - One sentence on *why this change*. The diff shows *what*; don't restate.
   - If an acceptance criterion is now satisfied, mention it.

6. **Stage only files relevant to this task.** Do not bundle unrelated changes; if you find unrelated work, leave it for a separate commit (or a separate task).

7. **Commit.**

   ```bash
   git commit -m "<subject> [T-N]" -m "<body>"
   ```

## Anti-patterns

- **Bundling tasks in one commit.** One commit per task ideally. If a task genuinely needs multiple commits (e.g. test commit + impl commit during TDD), each one carries `[T-N]`.
- **Squashing rationale into the subject.** Subject is the title; body is the why. Don't merge them.
- **Skipping `[T-N]`.** Without the tag, the cross-system link from `docket` to git is broken — `git log --grep='\[T-N\]'` is the canonical "what changed for this task" query.
- **Committing to "fix the test" mid-pursuit.** See `@tdd-pursuit.md` § Test-revision cheat. Test edits during pursuit are forbidden except for mechanical defects.

## Checklist

- [ ] Subject contains `[T-N]`
- [ ] Subject is imperative, ≤72 chars
- [ ] Body explains *why*, not *what*
- [ ] Only files relevant to T-N are staged
- [ ] If RED: subject starts with `test(...)`. If GREEN: starts with `feat(...)` or `fix(...)`.
