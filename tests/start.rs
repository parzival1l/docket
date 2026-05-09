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
            "docket-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        // make it look like a git repo so docket finds the root
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

    fn add(&self, title: &str, body: Option<&str>, acceptance: Option<&str>) -> String {
        let mut args: Vec<String> = vec!["add".into(), title.into()];
        if let Some(b) = body {
            args.push("--body".into());
            args.push(b.into());
        }
        if let Some(a) = acceptance {
            args.push("--acceptance".into());
            args.push(a.into());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(&refs);
        assert!(
            out.status.success(),
            "docket add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        // first whitespace-delimited token, e.g. "T-1"
        stdout.split_whitespace().next().unwrap().to_string()
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

fn tdd_template() -> String {
    let out = Command::new(bin()).args(["prompt", "tdd-pursuit"]).output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn start_prints_prompt_with_title_to_stdout() {
    let repo = TestRepo::new();
    let id = repo.add("Make widgets", Some("widget body"), Some("widgets exist"));

    let out = repo.run(&["start", &id]);

    assert!(
        out.status.success(),
        "expected success, got status {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Make widgets"),
        "stdout should contain task title; got:\n{}",
        stdout
    );
}

#[test]
fn start_prompt_contains_header_body_acceptance_and_tdd_in_order() {
    let repo = TestRepo::new();
    let id = repo.add(
        "Make widgets",
        Some("widget body content"),
        Some("widgets exist when invoked"),
    );

    let out = repo.run(&["start", &id]);
    assert!(
        out.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();

    let header = format!("# Task {}: Make widgets", id);
    let pos_header = stdout
        .find(&header)
        .unwrap_or_else(|| panic!("header `{}` not found in:\n{}", header, stdout));
    let pos_body_section = stdout
        .find("## Body")
        .unwrap_or_else(|| panic!("`## Body` section not found in:\n{}", stdout));
    let pos_body_text = stdout
        .find("widget body content")
        .unwrap_or_else(|| panic!("body content not found in:\n{}", stdout));
    let pos_accept_section = stdout
        .find("## Acceptance")
        .unwrap_or_else(|| panic!("`## Acceptance` section not found in:\n{}", stdout));
    let pos_accept_text = stdout
        .find("widgets exist when invoked")
        .unwrap_or_else(|| panic!("acceptance content not found in:\n{}", stdout));

    let tdd = tdd_template();
    let tdd_first_line = tdd.lines().next().unwrap();
    let pos_tdd = stdout
        .find(tdd_first_line)
        .unwrap_or_else(|| panic!("TDD template start `{}` not found in:\n{}", tdd_first_line, stdout));

    assert!(
        pos_header < pos_body_section
            && pos_body_section < pos_body_text
            && pos_body_text < pos_accept_section
            && pos_accept_section < pos_accept_text
            && pos_accept_text < pos_tdd,
        "sections out of order: header={} body_h={} body_t={} acc_h={} acc_t={} tdd={}\n{}",
        pos_header,
        pos_body_section,
        pos_body_text,
        pos_accept_section,
        pos_accept_text,
        pos_tdd,
        stdout
    );

    // PROMPT_TDD must appear verbatim (acceptance: "the full PROMPT_TDD template")
    assert!(
        stdout.contains(tdd.trim_end()),
        "verbatim TDD template not found in stdout"
    );
}

#[test]
fn start_transitions_status_to_in_progress_and_bumps_updated_at() {
    let repo = TestRepo::new();
    let id = repo.add("t", Some("b"), Some("a"));

    let before = show_json(&repo, &id);
    assert_eq!(before["status"], "open", "precondition: task starts open");
    let created_at = before["created_at"].as_str().unwrap().to_string();
    let updated_before = before["updated_at"].as_str().unwrap().to_string();
    assert_eq!(created_at, updated_before, "precondition: open task has updated_at == created_at");

    // ensure clock advances past timestamp resolution
    std::thread::sleep(std::time::Duration::from_millis(10));

    let out = repo.run(&["start", &id]);
    assert!(out.status.success(), "start failed: {}", String::from_utf8_lossy(&out.stderr));

    let after = show_json(&repo, &id);
    assert_eq!(after["status"], "in_progress", "status should transition to in_progress");
    let updated_after = after["updated_at"].as_str().unwrap().to_string();
    assert_ne!(
        updated_before, updated_after,
        "updated_at should be bumped after start"
    );
    assert_eq!(
        after["created_at"].as_str().unwrap(),
        created_at,
        "created_at must not change"
    );
}

#[test]
fn start_on_missing_id_fails_and_does_not_mutate_existing_task() {
    let repo = TestRepo::new();
    let real = repo.add("real", Some("b"), Some("a"));
    let real_before = show_json(&repo, &real);

    let out = repo.run(&["start", "T-99"]);
    assert!(
        !out.status.success(),
        "expected failure for missing ID; got success.\nstdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("T-99") || stderr.to_lowercase().contains("not found"),
        "stderr should explain missing ID; got:\n{}",
        stderr
    );

    // pre-existing task untouched
    let real_after = show_json(&repo, &real);
    assert_eq!(real_before["status"], real_after["status"]);
    assert_eq!(real_before["updated_at"], real_after["updated_at"]);
}

#[test]
fn start_on_done_task_fails_and_hints_at_docket_status() {
    let repo = TestRepo::new();
    let id = repo.add("t", Some("b"), Some("a"));
    let mark = repo.run(&["done", &id]);
    assert!(mark.status.success(), "done failed: {}", String::from_utf8_lossy(&mark.stderr));

    let out = repo.run(&["start", &id]);
    assert!(
        !out.status.success(),
        "expected failure when starting a done task; got success.\nstdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("docket status"),
        "stderr should hint at `docket status` to reopen; got:\n{}",
        stderr
    );

    // task still done — start did not silently mutate
    let after = show_json(&repo, &id);
    assert_eq!(after["status"], "done");
}

#[test]
fn start_on_already_in_progress_task_succeeds_and_reprints() {
    let repo = TestRepo::new();
    let id = repo.add("t", Some("b"), Some("a"));

    let first = repo.run(&["start", &id]);
    assert!(first.status.success());
    let first_stdout = String::from_utf8_lossy(&first.stdout).into_owned();

    let second = repo.run(&["start", &id]);
    assert!(
        second.status.success(),
        "second start on in_progress task should succeed; stderr:\n{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_stdout = String::from_utf8_lossy(&second.stdout).into_owned();

    // both invocations produced the assembled prompt (same shape)
    assert!(second_stdout.contains("# Task"), "second invocation should still print prompt");
    assert!(second_stdout.contains("## Body"));
    assert!(second_stdout.contains("## Acceptance"));
    assert_eq!(
        first_stdout, second_stdout,
        "re-running on in_progress should produce identical prompt body"
    );
}

#[test]
fn start_accepts_both_numeric_and_t_prefixed_ids() {
    let repo = TestRepo::new();
    let id_t = repo.add("t1", Some("b"), Some("a"));
    // strip "T-" → numeric form
    let id_num = id_t.trim_start_matches("T-").to_string();

    let with_prefix = repo.run(&["start", &id_t]);
    assert!(
        with_prefix.status.success(),
        "T- prefix form failed: {}",
        String::from_utf8_lossy(&with_prefix.stderr)
    );

    let with_numeric = repo.run(&["start", &id_num]);
    assert!(
        with_numeric.status.success(),
        "numeric form failed: {}",
        String::from_utf8_lossy(&with_numeric.stderr)
    );

    assert_eq!(
        String::from_utf8_lossy(&with_prefix.stdout),
        String::from_utf8_lossy(&with_numeric.stdout),
        "both ID forms should produce the same assembled prompt"
    );
}
