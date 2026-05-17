//! Hermetic end-to-end tests for the docket CLI.
//!
//! Each test creates its own tempdir-backed `.docket/` so they're parallel-safe.
//! Run with `cargo test -- --test-threads=8` to verify isolation.

use std::fs;
use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

/// A fresh repo with `.docket/` initialized. Drops the tempdir on drop.
struct Repo {
    tmp: TempDir,
}

impl Repo {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tempdir");
        // docket finds its root by walking up looking for `.git`
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        Command::cargo_bin("docket")
            .unwrap()
            .current_dir(tmp.path())
            .arg("init")
            .assert()
            .success();
        Repo { tmp }
    }

    fn path(&self) -> &std::path::Path {
        self.tmp.path()
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("docket").unwrap();
        c.current_dir(self.path());
        c
    }

    /// Run with args, return raw Output (does not assert success).
    fn run(&self, args: &[&str]) -> Output {
        self.cmd().args(args).output().expect("spawn docket")
    }

    /// Run an `add` with optional flags and return the printed `T-N` id.
    fn add(
        &self,
        title: &str,
        body: Option<&str>,
        acceptance: Option<&str>,
        deps: Option<&str>,
        priority: Option<i32>,
        group: Option<&str>,
    ) -> String {
        let mut args: Vec<String> = vec!["add".into(), title.into()];
        if let Some(b) = body {
            args.push("--body".into());
            args.push(b.into());
        }
        if let Some(a) = acceptance {
            args.push("--acceptance".into());
            args.push(a.into());
        }
        if let Some(d) = deps {
            args.push("--deps".into());
            args.push(d.into());
        }
        if let Some(p) = priority {
            args.push("--priority".into());
            args.push(p.to_string());
        }
        if let Some(g) = group {
            args.push("--group".into());
            args.push(g.into());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&refs);
        assert!(
            out.status.success(),
            "add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        // First whitespace-delimited token, e.g. "T-1".
        stdout.split_whitespace().next().unwrap().to_string()
    }

    /// Minimal add (title only).
    fn add_simple(&self, title: &str) -> String {
        self.add(title, None, None, None, None, None)
    }

    fn show_json(&self, id: &str) -> serde_json::Value {
        let out = self.run(&["show", id, "--json"]);
        assert!(
            out.status.success(),
            "show --json failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("show --json must emit valid JSON")
    }
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------- init ----------

#[test]
fn init_creates_docket_dir_and_sqlite_file() {
    let repo = Repo::new();
    let dir = repo.path().join(".docket");
    assert!(dir.exists(), "`.docket/` should exist after init");
    assert!(
        dir.join("db.sqlite").exists(),
        "`.docket/db.sqlite` should exist after init"
    );
}

// ---------- add ----------

#[test]
fn add_with_only_title_prints_t_prefixed_id() {
    let repo = Repo::new();
    repo.cmd()
        .args(["add", "Just a title"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("T-"));
}

#[test]
fn add_with_body_acceptance_deps_group_priority_round_trips() {
    let repo = Repo::new();
    let d1 = repo.add_simple("dep one");
    let d2 = repo.add_simple("dep two");
    let deps = format!("{},{}", d1, d2);
    let id = repo.add(
        "the thing",
        Some("the body"),
        Some("the accept"),
        Some(&deps),
        Some(1),
        Some("alpha"),
    );

    let j = repo.show_json(&id);
    assert_eq!(j["title"], "the thing");
    assert_eq!(j["body"], "the body");
    assert_eq!(j["acceptance"], "the accept");
    assert_eq!(j["priority"], 1);
    assert_eq!(j["group"], "alpha");
    let dep_ids: Vec<i64> = j["deps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let want = vec![
        d1.trim_start_matches("T-").parse::<i64>().unwrap(),
        d2.trim_start_matches("T-").parse::<i64>().unwrap(),
    ];
    assert_eq!(dep_ids, want);
}

#[test]
fn add_body_accepts_value_starting_with_hyphen() {
    let repo = Repo::new();
    let body = "- bullet one\n- bullet two";
    let id = repo.add("hyphen body", Some(body), None, None, None, None);
    let j = repo.show_json(&id);
    assert_eq!(j["body"], body);
}

#[test]
fn add_acceptance_accepts_value_starting_with_hyphen() {
    let repo = Repo::new();
    let accept = "- crit one\n- crit two";
    let id = repo.add("hyphen accept", None, Some(accept), None, None, None);
    let j = repo.show_json(&id);
    assert_eq!(j["acceptance"], accept);
}

#[test]
fn update_body_and_acceptance_accept_values_starting_with_hyphen() {
    let repo = Repo::new();
    let id = repo.add_simple("to update");
    let body = "- updated bullet one\n- updated bullet two";
    let accept = "- updated crit one\n- updated crit two";
    let out = repo.run(&[
        "update",
        &id,
        "--body",
        body,
        "--acceptance",
        accept,
    ]);
    assert!(
        out.status.success(),
        "update with leading-hyphen values failed: {}",
        stderr_of(&out)
    );
    let j = repo.show_json(&id);
    assert_eq!(j["body"], body);
    assert_eq!(j["acceptance"], accept);
}

#[test]
fn short_help_and_version_flags_still_work() {
    let repo = Repo::new();
    let h = repo.run(&["-h"]);
    assert!(h.status.success(), "-h should succeed");
    let h_out = stdout_of(&h);
    assert!(
        h_out.to_lowercase().contains("usage"),
        "-h output should look like help; got:\n{}",
        h_out
    );

    let v = repo.run(&["-V"]);
    assert!(v.status.success(), "-V should succeed");
    let v_out = stdout_of(&v);
    assert!(
        v_out.contains("docket"),
        "-V should print version with binary name; got:\n{}",
        v_out
    );
}

// ---------- add --kind ----------

#[test]
fn add_without_kind_defaults_to_feature() {
    let repo = Repo::new();
    let id = repo.add_simple("default kind");
    let j = repo.show_json(&id);
    assert_eq!(
        j["kind"], "feature",
        "tasks created without --kind should default to feature"
    );
}

#[test]
fn add_with_kind_bug_round_trips_through_show_json() {
    let repo = Repo::new();
    let out = repo.run(&["add", "a clap bug", "--kind", "bug"]);
    assert!(
        out.status.success(),
        "add --kind bug failed: {}",
        stderr_of(&out)
    );
    let id = stdout_of(&out)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let j = repo.show_json(&id);
    assert_eq!(j["kind"], "bug");
}

#[test]
fn add_with_unknown_kind_errors_and_lists_vocabulary() {
    let repo = Repo::new();
    let out = repo.run(&["add", "broken", "--kind", "nonsense"]);
    assert!(
        !out.status.success(),
        "add --kind nonsense should fail; got success"
    );
    let err = stderr_of(&out).to_lowercase();
    for k in &["bug", "feature", "chore", "docs", "spike"] {
        assert!(
            err.contains(k),
            "error message should list `{}`; got:\n{}",
            k,
            err
        );
    }
}

#[test]
fn add_accepts_all_documented_kinds() {
    let repo = Repo::new();
    for k in &["bug", "feature", "chore", "docs", "spike"] {
        let out = repo.run(&["add", &format!("a {}", k), "--kind", k]);
        assert!(
            out.status.success(),
            "add --kind {} failed: {}",
            k,
            stderr_of(&out)
        );
        let id = stdout_of(&out)
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        assert_eq!(repo.show_json(&id)["kind"], *k);
    }
}

#[test]
fn ls_kind_filter_excludes_other_kinds() {
    let repo = Repo::new();
    let bug_out = repo.run(&["add", "the bug", "--kind", "bug"]);
    assert!(bug_out.status.success());
    let bug_id = stdout_of(&bug_out)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let feat_id = repo.add_simple("the feature");

    let out = repo.run(&["ls", "--kind", "bug"]);
    assert!(out.status.success(), "ls --kind bug failed: {}", stderr_of(&out));
    let s = stdout_of(&out);
    assert!(s.contains(&bug_id), "bug id should be listed; got:\n{}", s);
    assert!(
        !s.contains(&feat_id),
        "feature id should be filtered out; got:\n{}",
        s
    );
}

#[test]
fn ls_default_output_uses_kind_shorthand_without_brackets() {
    let repo = Repo::new();
    let feat_id = repo.add_simple("a feature task");
    let bug_out = repo.run(&["add", "a bug task", "--kind", "bug"]);
    assert!(bug_out.status.success());
    let bug_id = stdout_of(&bug_out)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    let out = repo.run(&["ls"]);
    let s = stdout_of(&out);

    let feat_row = s
        .lines()
        .find(|l| l.contains(&feat_id))
        .unwrap_or_else(|| panic!("no ls row for {}; got:\n{}", feat_id, s));
    assert!(
        feat_row.contains("fea") && !feat_row.contains("[feature]"),
        "feature row should use `fea` shorthand without brackets; got:\n{}",
        feat_row
    );

    let bug_row = s
        .lines()
        .find(|l| l.contains(&bug_id))
        .unwrap_or_else(|| panic!("no ls row for {}; got:\n{}", bug_id, s));
    assert!(
        bug_row.contains("bug") && !bug_row.contains("[bug]"),
        "bug row should use `bug` shorthand without brackets; got:\n{}",
        bug_row
    );
}

#[test]
fn ls_default_output_surfaces_kind() {
    let repo = Repo::new();
    let bug_out = repo.run(&["add", "the bug", "--kind", "bug"]);
    assert!(bug_out.status.success());
    let bug_id = stdout_of(&bug_out)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    let out = repo.run(&["ls"]);
    let s = stdout_of(&out);
    // The bug task's row must surface its kind so a scanner can see it.
    let row = s
        .lines()
        .find(|l| l.contains(&bug_id))
        .unwrap_or_else(|| panic!("no ls row for {}; got:\n{}", bug_id, s));
    assert!(
        row.contains("bug"),
        "ls row for {} should mention kind `bug`; got:\n{}",
        bug_id,
        row
    );
}

#[test]
fn show_text_output_includes_kind() {
    let repo = Repo::new();
    let out = repo.run(&["add", "the bug", "--kind", "bug"]);
    assert!(out.status.success());
    let id = stdout_of(&out)
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();
    let out = repo.run(&["show", &id]);
    let s = stdout_of(&out);
    assert!(
        s.contains("bug"),
        "show text output should mention kind `bug`; got:\n{}",
        s
    );
}

#[test]
fn add_with_invalid_deps_string_errors() {
    let repo = Repo::new();
    repo.cmd()
        .args(["add", "broken", "--deps", "T-notanumber"])
        .assert()
        .failure();
}

// ---------- ls ----------

#[test]
fn ls_on_empty_repo_says_no_tasks() {
    let repo = Repo::new();
    repo.cmd()
        .args(["ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(no tasks)"));
}

#[test]
fn ls_lists_added_tasks_with_t_prefix_and_status() {
    let repo = Repo::new();
    let id = repo.add_simple("hello world");
    repo.cmd()
        .args(["ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&id).and(predicate::str::contains("open")))
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn ls_status_filter_excludes_other_statuses() {
    let repo = Repo::new();
    let open_id = repo.add_simple("still open");
    let done_id = repo.add_simple("will be done");
    let mark = repo.run(&["done", &done_id]);
    assert!(mark.status.success());

    let out = repo.run(&["ls", "--status", "open"]);
    assert!(out.status.success());
    let s = stdout_of(&out);
    assert!(s.contains(&open_id), "open id should be listed; got:\n{}", s);
    assert!(!s.contains(&done_id), "done id should be filtered out; got:\n{}", s);
}

#[test]
fn ls_group_filter_only_returns_matching_group() {
    let repo = Repo::new();
    let in_group = repo.add("inside", None, None, None, None, Some("g1"));
    let out_group = repo.add_simple("outside");

    let out = repo.run(&["ls", "--group", "g1"]);
    let s = stdout_of(&out);
    assert!(s.contains(&in_group));
    assert!(!s.contains(&out_group));
}

#[test]
fn ls_json_emits_valid_array() {
    let repo = Repo::new();
    repo.add_simple("a");
    repo.add_simple("b");
    let out = repo.run(&["ls", "--json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("ls --json should emit JSON");
    let arr = v.as_array().expect("ls --json should emit an array");
    assert_eq!(arr.len(), 2);
}

// ---------- show ----------

#[test]
fn show_existing_id_prints_title_and_status() {
    let repo = Repo::new();
    let id = repo.add_simple("widget");
    repo.cmd()
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("widget").and(predicate::str::contains("open")));
}

#[test]
fn show_accepts_bare_numeric_id() {
    let repo = Repo::new();
    let id_t = repo.add_simple("numeric form");
    let num = id_t.trim_start_matches("T-");

    let with_prefix = repo.run(&["show", &id_t, "--json"]);
    let with_num = repo.run(&["show", num, "--json"]);
    assert!(with_prefix.status.success() && with_num.status.success());
    assert_eq!(with_prefix.stdout, with_num.stdout);
}

#[test]
fn show_missing_id_errors_with_not_found() {
    let repo = Repo::new();
    let out = repo.run(&["show", "T-999"]);
    assert!(!out.status.success());
    let err = stderr_of(&out).to_lowercase();
    assert!(
        err.contains("not found") || err.contains("t-999"),
        "expected `not found` message; got:\n{}",
        err
    );
}

// ---------- ready ----------

#[test]
fn ready_lists_open_tasks_with_no_unmet_deps() {
    let repo = Repo::new();
    let id = repo.add_simple("standalone");
    repo.cmd()
        .args(["ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&id));
}

#[test]
fn ready_omits_tasks_whose_dep_is_not_done() {
    let repo = Repo::new();
    let dep = repo.add_simple("dep");
    let dependent = repo.add("depends on dep", None, None, Some(&dep), None, None);

    let out = repo.run(&["ready"]);
    let s = stdout_of(&out);
    assert!(s.contains(&dep), "dep itself should be ready; got:\n{}", s);
    assert!(
        !s.contains(&dependent),
        "dependent task should NOT be ready while dep is open; got:\n{}",
        s
    );
}

#[test]
fn ready_includes_dependent_once_dep_is_done() {
    let repo = Repo::new();
    let dep = repo.add_simple("dep");
    let dependent = repo.add("depends on dep", None, None, Some(&dep), None, None);
    assert!(repo.run(&["done", &dep]).status.success());

    let out = repo.run(&["ready"]);
    let s = stdout_of(&out);
    assert!(
        s.contains(&dependent),
        "dependent task should be ready once dep is done; got:\n{}",
        s
    );
}

#[test]
fn ready_group_filter_excludes_other_groups() {
    let repo = Repo::new();
    let in_g = repo.add("in g", None, None, None, None, Some("g"));
    let out_g = repo.add_simple("out");
    let out = repo.run(&["ready", "--group", "g"]);
    let s = stdout_of(&out);
    assert!(s.contains(&in_g));
    assert!(!s.contains(&out_g));
}

// ---------- blocked ----------

#[test]
fn blocked_lists_tasks_with_unmet_deps() {
    let repo = Repo::new();
    let dep = repo.add_simple("dep");
    let dependent = repo.add("dependent", None, None, Some(&dep), None, None);

    let out = repo.run(&["blocked"]);
    let s = stdout_of(&out);
    assert!(s.contains(&dependent), "blocked should list dependent; got:\n{}", s);
    // Unmet dep should be surfaced as well.
    assert!(s.contains(&dep), "blocked output should mention dep; got:\n{}", s);
}

#[test]
fn blocked_says_none_when_all_deps_satisfied() {
    let repo = Repo::new();
    repo.add_simple("standalone");
    repo.cmd()
        .args(["blocked"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(no blocked tasks)"));
}

// ---------- status ----------

#[test]
fn status_sets_arbitrary_state_string() {
    let repo = Repo::new();
    let id = repo.add_simple("t");
    repo.cmd()
        .args(["status", &id, "review"])
        .assert()
        .success();

    let j = repo.show_json(&id);
    assert_eq!(j["status"], "review");
}

#[test]
fn status_transitions_open_to_in_progress_to_done() {
    let repo = Repo::new();
    let id = repo.add_simple("t");
    assert!(repo.run(&["status", &id, "in_progress"]).status.success());
    assert_eq!(repo.show_json(&id)["status"], "in_progress");
    assert!(repo.run(&["status", &id, "done"]).status.success());
    assert_eq!(repo.show_json(&id)["status"], "done");
}

#[test]
fn status_missing_id_errors() {
    let repo = Repo::new();
    repo.cmd()
        .args(["status", "T-999", "done"])
        .assert()
        .failure();
}

// ---------- done ----------

#[test]
fn done_sets_status_to_done() {
    let repo = Repo::new();
    let id = repo.add_simple("t");
    repo.cmd().args(["done", &id]).assert().success();
    assert_eq!(repo.show_json(&id)["status"], "done");
}

#[test]
fn done_is_idempotent() {
    let repo = Repo::new();
    let id = repo.add_simple("t");
    assert!(repo.run(&["done", &id]).status.success());
    // Re-running done on an already-done task is allowed; status stays done.
    assert!(repo.run(&["done", &id]).status.success());
    assert_eq!(repo.show_json(&id)["status"], "done");
}

#[test]
fn done_missing_id_errors() {
    let repo = Repo::new();
    repo.cmd().args(["done", "T-999"]).assert().failure();
}

// ---------- rm ----------

#[test]
fn rm_removes_existing_task() {
    let repo = Repo::new();
    let id = repo.add_simple("doomed");
    repo.cmd().args(["rm", &id]).assert().success();
    let out = repo.run(&["show", &id]);
    assert!(!out.status.success(), "show after rm should error");
}

#[test]
fn rm_missing_id_errors() {
    let repo = Repo::new();
    repo.cmd().args(["rm", "T-999"]).assert().failure();
}

// ---------- start (basic; tmux paths covered in tests/start.rs) ----------

#[test]
fn start_prints_assembled_prompt_and_flips_status() {
    let repo = Repo::new();
    let id = repo.add(
        "build a thing",
        Some("body text"),
        Some("accept text"),
        None,
        None,
        None,
    );

    // Pull the tdd template for substring check.
    let tdd_out = repo
        .cmd()
        .args(["prompt", "tdd-pursuit"])
        .output()
        .expect("spawn docket");
    assert!(tdd_out.status.success());
    let tdd = String::from_utf8_lossy(&tdd_out.stdout);
    let tdd_first_line = tdd.lines().next().expect("PROMPT_TDD must have at least one line");

    let out = repo.run(&["start", &id]);
    assert!(
        out.status.success(),
        "start failed: {}",
        stderr_of(&out)
    );
    let s = stdout_of(&out);
    assert!(s.contains(&format!("# Task {}: build a thing", id)));
    assert!(s.contains("## Body"));
    assert!(s.contains("body text"));
    assert!(s.contains("## Acceptance"));
    assert!(s.contains("accept text"));
    assert!(
        s.contains(tdd_first_line),
        "PROMPT_TDD first line `{}` should appear in start output",
        tdd_first_line
    );

    assert_eq!(repo.show_json(&id)["status"], "in_progress");
}

#[test]
fn start_on_done_task_errors_and_leaves_status_unchanged() {
    let repo = Repo::new();
    let id = repo.add_simple("t");
    assert!(repo.run(&["done", &id]).status.success());
    let before = repo.show_json(&id);

    let out = repo.run(&["start", &id]);
    assert!(!out.status.success(), "starting a done task must error");

    let after = repo.show_json(&id);
    assert_eq!(after["status"], "done", "status must remain done");
    assert_eq!(
        after["updated_at"], before["updated_at"],
        "updated_at must not change when start is rejected"
    );
}

#[test]
fn start_missing_id_errors() {
    let repo = Repo::new();
    repo.cmd().args(["start", "T-999"]).assert().failure();
}

// ---------- group new / ls / show / close ----------

#[test]
fn group_new_creates_visible_group() {
    let repo = Repo::new();
    repo.cmd()
        .args(["group", "new", "v0.1", "--branch", "release/v0.1"])
        .assert()
        .success();
    repo.cmd()
        .args(["group", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("v0.1"));
}

#[test]
fn group_new_rejects_duplicate_name() {
    let repo = Repo::new();
    repo.cmd().args(["group", "new", "dup"]).assert().success();
    repo.cmd().args(["group", "new", "dup"]).assert().failure();
}

#[test]
fn group_ls_says_none_when_empty() {
    let repo = Repo::new();
    repo.cmd()
        .args(["group", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(no groups)"));
}

#[test]
fn group_show_lists_member_tasks() {
    let repo = Repo::new();
    repo.cmd().args(["group", "new", "g"]).assert().success();
    let in_g = repo.add("in group", None, None, None, None, Some("g"));
    let out_g = repo.add_simple("not in group");

    let out = repo.run(&["group", "show", "g"]);
    assert!(out.status.success());
    let s = stdout_of(&out);
    assert!(s.contains(&in_g), "member task missing; got:\n{}", s);
    assert!(!s.contains(&out_g), "non-member task should not appear; got:\n{}", s);
}

#[test]
fn group_show_missing_group_errors() {
    let repo = Repo::new();
    repo.cmd()
        .args(["group", "show", "ghost"])
        .assert()
        .failure();
}

#[test]
fn group_close_marks_state_closed() {
    let repo = Repo::new();
    repo.cmd().args(["group", "new", "g"]).assert().success();
    repo.cmd().args(["group", "close", "g"]).assert().success();

    let out = repo.run(&["group", "ls", "--json"]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let g = v
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["name"] == "g")
        .expect("group g should be in ls output");
    assert_eq!(g["state"], "closed");
}

#[test]
fn group_close_missing_group_errors() {
    let repo = Repo::new();
    repo.cmd()
        .args(["group", "close", "ghost"])
        .assert()
        .failure();
}

// ---------- isolation guard ----------

// Sanity check that two repos created in the same test really don't share state.
#[test]
fn two_repos_in_one_test_are_isolated() {
    let r1 = Repo::new();
    let r2 = Repo::new();
    let _id1 = r1.add_simple("only in r1");
    let out = r2.run(&["ls"]);
    let s = stdout_of(&out);
    assert!(
        s.contains("(no tasks)"),
        "r2 should not see r1's tasks; got:\n{}",
        s
    );
    // also: their .docket/db.sqlite are different files
    let _: PathBuf = r1.path().join(".docket/db.sqlite");
    assert_ne!(
        r1.path().join(".docket/db.sqlite"),
        r2.path().join(".docket/db.sqlite")
    );
}
