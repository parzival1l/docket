# TDD Pursuit

You are picking up a task that has two fixed inputs:

- **`body`** — the task description (intent)
- **`acceptance`** — testable criteria (definition of done)

Encode `acceptance` as one or more tests, then make those tests pass. The task and the test are the contract; the implementation is the variable.

## The loop

Work in vertical slices, one behavior at a time. Do **not** write all tests up front.

For each behavior in `acceptance`:

1. **RED** — write one test that encodes the behavior. Run it. Confirm it fails for the right reason (missing/wrong implementation, not a typo or syntax error).
2. **GREEN** — write the minimum implementation that makes the test pass. No speculative features. No code for tests you haven't written yet.
3. **REFACTOR** *(optional, only when green)* — improve the implementation without changing observable behavior. Re-run the suite after each step. If a refactor turns it red, revert.

Repeat until every item in `acceptance` is covered.

## What "the test" encodes

The test is `acceptance` translated into executable form. It describes **what** the system does through its public interface, not **how** it does it. Prefer integration-style tests that exercise the real path. Mock only at true system boundaries (network, clock, randomness, third-party SDKs) — never your own modules.

A good test survives an internal refactor. If renaming a private helper breaks the test, the test is wrong shape.

## Anti-cheating disciplines

These are failure modes you must recognize **by name** mid-loop. If you catch yourself doing one, stop and escalate.

### 1. Task-revision cheat
Editing `body` or `acceptance` so it matches what you actually built. **Forbidden.** The task is upstream of you. If the task is wrong, that's a planning-layer bug.

### 2. Test-revision cheat
Weakening, deleting, or rewriting the test so a buggy implementation passes. **Forbidden.** Examples:

- Changing an expected value to match what the code happens to return.
- Loosening an assertion (`toEqual` → `toBeTruthy`, removing fields, widening tolerances) without a behavior reason in `acceptance`.
- Deleting a failing case "because it's edge."
- Wrapping a failing assertion in `try/catch` or skipping it.

If the test is wrong, that's also a planning-layer bug. Fix it at planning, not mid-pursuit.

### 3. Tautology cheat
Writing a test that cannot fail, or asserts the implementation against itself (`expect(fn(x)).toBe(fn(x))`). The RED step exists to catch this — if the first run of a brand-new test passes, the test is broken.

### 4. Implementation-leak cheat
Asserting on private state, internal call counts, or log strings instead of observable behavior. Symptom: the test passes but user-visible behavior is still wrong, or the test fails on harmless refactors.

### 5. Horizontal-slice cheat
Writing all tests first, then all implementation. Produces tests that describe imagined behavior. Stay vertical: one test, one implementation, then the next.

## Modifying the test vs. the implementation

These are **not symmetric**. Be explicit on every edit about which one you're doing.

- **Modifying the implementation** — this is the entire job. Iterate freely.
- **Modifying the test** — forbidden during pursuit, with two narrow exceptions:
  1. The test has a **mechanical defect** (typo, wrong import, wrong fixture path, syntax error) that prevents it from running at all. Fix the defect; do not change what is asserted.
  2. You are in REFACTOR phase, the suite is green, and the change is purely structural (renaming a test, extracting a helper, deduping setup). Assertions and inputs stay identical.

Anything else — changing inputs, expected values, or which behavior is asserted — is a test-revision cheat. Escalate instead.

## When to escalate to the human

Legitimate exits from the loop, not failures. Stop and ask:

- **Acceptance is ambiguous.** You cannot translate a criterion into a test without guessing.
- **Acceptance is internally inconsistent.** Two criteria contradict each other.
- **The test seems wrong.** You wrote it honestly from `acceptance`, but on reflection it encodes behavior the user almost certainly didn't intend, or it conflicts with the surrounding system.
- **`acceptance` requires changes outside task scope** — new dependencies, schema migrations, cross-cutting API changes.
- **Same failure ≥3 attempts.** Something upstream is wrong: design, acceptance, or your model of the system.

When escalating, surface: which criterion, what you tried, what you think the planning-layer fix is. Do **not** silently patch the test or the task.

## Per-cycle checklist

### Before writing the test
- [ ] You can name the specific behavior from `acceptance` you're about to encode.
- [ ] The test will assert observable behavior, not internal mechanics.

### After RED
- [ ] The test failed.
- [ ] It failed for the right reason (missing/wrong impl), not a defect in the test itself.

### After GREEN
- [ ] Implementation is the smallest change that passes.
- [ ] No other test went red.
- [ ] `body` and `acceptance` are unchanged from when you picked up the task.
- [ ] The test file has not been edited except per the rules above.
