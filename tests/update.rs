use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_docket")
}

static SEQ: AtomicU64 = AtomicU64::new(0);

struct TestRepo {
    dir: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!(
            "docket-update-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        let out = Command::new(bin())
            .arg("init")
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "docket init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        TestRepo { dir }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(bin())
            .args(args)
            .current_dir(&self.dir)
            .output()
            .unwrap()
    }

    fn add_full(
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
            "docket add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout.split_whitespace().next().unwrap().to_string()
    }

    fn add(&self, title: &str) -> String {
        self.add_full(title, None, None, None, None, None)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn show_json(repo: &TestRepo, id: &str) -> serde_json::Value {
    let out = repo.run(&["show", id, "--json"]);
    assert!(
        out.status.success(),
        "show --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("show --json produced invalid JSON")
}

#[test]
fn update_subcommand_visible_in_top_level_help() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("update"), "`update` should appear in --help; got:\n{}", s);
}

#[test]
fn update_help_lists_all_field_flags() {
    let out = Command::new(bin()).args(["update", "--help"]).output().unwrap();
    assert!(out.status.success(), "update --help failed: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    for flag in &["--title", "--body", "--acceptance", "--deps", "--priority", "--group"] {
        assert!(
            s.contains(flag),
            "`update --help` should list {}; got:\n{}",
            flag,
            s
        );
    }
}

#[test]
fn update_deps_replaces_deps_list() {
    let repo = TestRepo::new();
    let d1 = repo.add("dep-one");
    let d2 = repo.add("dep-two");
    let d3 = repo.add("dep-three");
    // task starts with a single dep
    let id = repo.add_full("with deps", None, None, Some(&d1), None, None);

    let before = show_json(&repo, &id);
    assert_eq!(before["deps"].as_array().unwrap().len(), 1);

    let new_deps = format!("{},{}", d2, d3);
    let out = repo.run(&["update", &id, "--deps", &new_deps]);
    assert!(
        out.status.success(),
        "update --deps failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = show_json(&repo, &id);
    let deps = after["deps"].as_array().unwrap();
    let dep_ids: Vec<i64> = deps.iter().map(|v| v.as_i64().unwrap()).collect();
    let want = vec![
        d2.trim_start_matches("T-").parse::<i64>().unwrap(),
        d3.trim_start_matches("T-").parse::<i64>().unwrap(),
    ];
    assert_eq!(
        dep_ids, want,
        "deps should be replaced exactly, not merged with the old dep ({})",
        d1
    );
}

#[test]
fn update_group_creates_lazily_and_reassigns_task() {
    let repo = TestRepo::new();
    let id = repo.add_full("t", None, None, None, None, None);
    let before = show_json(&repo, &id);
    assert!(
        before["group"].is_null(),
        "precondition: task starts with no group; got {:?}",
        before["group"]
    );

    let out = repo.run(&["update", &id, "--group", "brand-new"]);
    assert!(
        out.status.success(),
        "update --group failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = show_json(&repo, &id);
    assert_eq!(
        after["group"], "brand-new",
        "task should now belong to brand-new group"
    );

    // the group is visible in `docket group ls`
    let g_out = repo.run(&["group", "ls"]);
    assert!(g_out.status.success());
    let g_stdout = String::from_utf8_lossy(&g_out.stdout);
    assert!(
        g_stdout.contains("brand-new"),
        "brand-new group should appear in `group ls` output; got:\n{}",
        g_stdout
    );
}

#[test]
fn update_missing_id_errors_and_does_not_mutate_other_tasks() {
    let repo = TestRepo::new();
    let id = repo.add_full(
        "the title",
        Some("the body"),
        Some("the acceptance"),
        None,
        Some(2),
        None,
    );
    let before = show_json(&repo, &id);
    let updated_before = before["updated_at"].as_str().unwrap().to_string();

    let out = repo.run(&["update", "T-99", "--priority", "0"]);
    assert!(
        !out.status.success(),
        "expected non-zero exit for missing id, got success.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let after = show_json(&repo, &id);
    assert_eq!(after["title"], "the title");
    assert_eq!(after["priority"], 2);
    assert_eq!(
        after["updated_at"].as_str().unwrap(),
        updated_before,
        "updated_at on the existing task must not change"
    );
}

#[test]
fn update_with_no_field_flags_errors_non_zero() {
    let repo = TestRepo::new();
    let id = repo.add_full(
        "old title",
        Some("old body"),
        Some("ax"),
        None,
        Some(2),
        None,
    );
    let before = show_json(&repo, &id);
    let updated_before = before["updated_at"].as_str().unwrap().to_string();

    let out = repo.run(&["update", &id]);
    assert!(
        !out.status.success(),
        "expected non-zero exit for no-flag update, got success.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("at least one")
            || stderr.to_lowercase().contains("no fields"),
        "expected a clear error mentioning that at least one field is required; got: {}",
        stderr
    );

    // no mutation occurred
    let after = show_json(&repo, &id);
    assert_eq!(after["title"], "old title");
    assert_eq!(after["body"], "old body");
    assert_eq!(after["acceptance"], "ax");
    assert_eq!(after["priority"], 2);
    assert_eq!(
        after["updated_at"].as_str().unwrap(),
        updated_before,
        "updated_at must not change when update is rejected"
    );
}

#[test]
fn update_title_and_body_in_one_call() {
    let repo = TestRepo::new();
    let id = repo.add_full(
        "old title",
        Some("old body"),
        Some("the acceptance"),
        None,
        Some(2),
        None,
    );

    let out = repo.run(&["update", &id, "--title", "new", "--body", "new body"]);
    assert!(
        out.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = show_json(&repo, &id);
    assert_eq!(after["title"], "new");
    assert_eq!(after["body"], "new body");
    assert_eq!(after["acceptance"], "the acceptance", "acceptance untouched");
    assert_eq!(after["priority"], 2, "priority untouched");
}

#[test]
fn update_priority_changes_priority_and_bumps_updated_at_only() {
    let repo = TestRepo::new();
    let id = repo.add_full(
        "the title",
        Some("the body"),
        Some("the acceptance"),
        None,
        Some(2),
        None,
    );

    let before = show_json(&repo, &id);
    let created_at = before["created_at"].as_str().unwrap().to_string();
    let updated_before = before["updated_at"].as_str().unwrap().to_string();
    assert_eq!(before["priority"], 2);

    std::thread::sleep(std::time::Duration::from_millis(10));

    let out = repo.run(&["update", &id, "--priority", "1"]);
    assert!(
        out.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = show_json(&repo, &id);
    assert_eq!(after["priority"], 1, "priority should be updated");
    assert_eq!(after["title"], "the title", "title untouched");
    assert_eq!(after["body"], "the body", "body untouched");
    assert_eq!(
        after["acceptance"], "the acceptance",
        "acceptance untouched"
    );
    assert_eq!(
        after["created_at"].as_str().unwrap(),
        created_at,
        "created_at must not change"
    );
    assert_ne!(
        after["updated_at"].as_str().unwrap(),
        updated_before,
        "updated_at must be bumped"
    );
}

#[test]
fn update_kind_changes_kind_field() {
    let repo = TestRepo::new();
    let id = repo.add("a feature");
    // default kind is feature
    assert_eq!(show_json(&repo, &id)["kind"], "feature");

    let out = repo.run(&["update", &id, "--kind", "bug"]);
    assert!(
        out.status.success(),
        "update --kind bug failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(show_json(&repo, &id)["kind"], "bug");
}

#[test]
fn update_kind_rejects_unknown_value() {
    let repo = TestRepo::new();
    let id = repo.add("t");
    let out = repo.run(&["update", &id, "--kind", "nonsense"]);
    assert!(
        !out.status.success(),
        "update --kind nonsense should fail; got success"
    );
    let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
    for k in &["bug", "feature", "chore", "docs", "spike"] {
        assert!(
            err.contains(k),
            "error message should list `{}`; got:\n{}",
            k,
            err
        );
    }
    // kind must not have changed on rejection.
    assert_eq!(show_json(&repo, &id)["kind"], "feature");
}
