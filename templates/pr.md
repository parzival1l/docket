# PR (group complete)

You are creating a pull request after completing a `docket` group. One PR represents one group: multiple tasks, one branch, one PR.

## Inputs

- The group name and its tasks (`docket group show <name>`)
- Git log on the current branch (`git log <base>..HEAD --oneline`)
- Commits, each tagged `[T-N]` per `@commit.md`

## Output

A PR title and body that lets a reviewer understand what changed and why without reading every commit.

## Steps

1. **Confirm the group is complete.** Every task in the group has `status='done'`. If any task is still `open` or `in_progress`, **stop** — don't open a PR for an incomplete group.

   ```bash
   docket group show <name> --json   # verify all tasks are done
   ```

2. **Read each task's body and acceptance.** This is the *what* and *why* of the PR. The PR body aggregates these, not the commit messages.

3. **Write the title.** ≤70 chars. Use the group name and a one-line summary. If the group has a single dominant theme, lead with it.

   - `auth-rewrite: token refresh flow + RBAC enforcement`
   - `uploader: retry semantics for partial-failure batches`

4. **Write the body.** Structure:

   ```markdown
   ## Summary
   <2-3 sentences. What ships in this PR and why now.>

   ## Tasks
   - [T-N] <task title> — <one-line outcome>
   - [T-M] <task title> — <one-line outcome>

   ## Test plan
   <Bulleted list of what to verify in review or post-merge.>

   ## Notes for reviewers
   <Optional: anything non-obvious about the approach, gotchas, deferred work.>
   ```

5. **Open the PR** with `gh pr create` (or your usual flow). Do not auto-merge.

## Anti-patterns

- **Per-task PRs.** Defeats the purpose of grouping. If a single task is big enough to warrant its own PR, it shouldn't have been in a group.
- **Generated commit-list bodies.** A `git log --oneline` dump is not a PR description. The body is for the reviewer; the commits are the diff.
- **Soft acceptance.** If a task's acceptance was relaxed during execution (it shouldn't have been per `@tdd-pursuit.md`), flag it explicitly in "Notes for reviewers" so the reviewer can decide whether to push back.

## Checklist

- [ ] All group tasks are `done` in `docket`
- [ ] Every commit on the branch carries `[T-N]` for some task in the group
- [ ] PR title ≤70 chars
- [ ] PR body has Summary, Tasks (with T-N references), Test plan
- [ ] No unrelated commits on the branch
