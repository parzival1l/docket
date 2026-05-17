use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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

#[cfg(unix)]
struct FakeTmux {
    bin_dir: PathBuf,
    log_path: PathBuf,
}

#[cfg(unix)]
impl FakeTmux {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let bin_dir = env::temp_dir().join(format!(
            "docket-faketmux-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        fs::create_dir_all(&bin_dir).unwrap();
        let log_path = bin_dir.join("calls.log");
        let script_path = bin_dir.join("tmux");
        // The fake tmux records the full argv (one call per line, tab-delimited)
        // and emits a fake window/session id when invoked with `new-window -P`
        // or `new-session -P`.
        let script = format!(
            "#!/bin/sh\n\
LOG='{log}'\n\
{{ printf 'CALL'; for a in \"$@\"; do printf '\\t%s' \"$a\"; done; printf '\\n'; }} >> \"$LOG\"\n\
case \"$1\" in\n\
  new-window)\n\
    for a in \"$@\"; do\n\
      if [ \"$a\" = \"-P\" ]; then echo '@99'; break; fi\n\
    done\n\
    ;;\n\
  new-session)\n\
    for a in \"$@\"; do\n\
      if [ \"$a\" = \"-P\" ]; then echo '$99'; break; fi\n\
    done\n\
    ;;\n\
esac\n\
exit 0\n",
            log = log_path.display()
        );
        fs::write(&script_path, script).unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
        FakeTmux { bin_dir, log_path }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        if !self.log_path.exists() {
            return vec![];
        }
        let content = fs::read_to_string(&self.log_path).unwrap();
        content
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let stripped = l.strip_prefix("CALL\t").unwrap_or(l.strip_prefix("CALL").unwrap_or(l));
                stripped.split('\t').map(String::from).collect()
            })
            .collect()
    }
}

#[cfg(unix)]
impl Drop for FakeTmux {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.bin_dir);
    }
}

#[cfg(unix)]
fn run_with_tmux(repo: &TestRepo, args: &[&str], fake: &FakeTmux) -> Output {
    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fake.bin_dir.display(), original_path);
    Command::new(bin())
        .args(args)
        .current_dir(&repo.dir)
        .env("PATH", new_path)
        .env("TMUX", "/tmp/fake-tmux-socket,12345,0")
        .output()
        .unwrap()
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

#[cfg(unix)]
#[test]
fn start_tmux_outside_tmux_spawns_terminal_with_attach() {
    let repo = TestRepo::new();
    let id = repo.add("widgets", Some("body content"), Some("acceptance"));

    let fake = FakeTmux::new();
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let spawn_log = env::temp_dir().join(format!(
        "docket-spawn-{}-{}.log",
        std::process::id(),
        n
    ));
    // The fake spawner just records the expanded attach command. Using a
    // single-quoted shell snippet avoids surprises with how {cmd} substitutes.
    let spawn_cmd = format!("printf '%s\\n' \"{{cmd}}\" >> {}", spawn_log.display());

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fake.bin_dir.display(), original_path);
    let out = Command::new(bin())
        .args(["start", &id, "--tmux"])
        .current_dir(&repo.dir)
        .env("PATH", new_path)
        .env_remove("TMUX")
        .env("DOCKET_TERMINAL_CMD", &spawn_cmd)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "outside-tmux --tmux should spawn a terminal and succeed; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let calls = fake.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(|s| s.as_str()) == Some("new-session")),
        "expected `tmux new-session` call; got: {:#?}",
        calls
    );
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(|s| s.as_str()) == Some("send-keys")),
        "expected `tmux send-keys` call; got: {:#?}",
        calls
    );
    assert!(
        !calls
            .iter()
            .any(|c| c.first().map(|s| s.as_str()) == Some("switch-client")),
        "switch-client should NOT be called when $TMUX is unset; got: {:#?}",
        calls
    );

    // Spawner was invoked with `tmux attach -t docket-T-N-<ts>`
    let spawn_content = fs::read_to_string(&spawn_log).unwrap_or_default();
    assert!(
        spawn_content.contains("tmux attach -t docket-"),
        "terminal spawner should have been invoked with attach command; got:\n{}",
        spawn_content
    );

    // Status flipped on success
    let after = show_json(&repo, &id);
    assert_eq!(
        after["status"], "in_progress",
        "status should flip to in_progress when --tmux succeeds via spawn"
    );

    let _ = fs::remove_file(&spawn_log);
}

#[cfg(unix)]
#[test]
fn start_tmux_outside_no_spawn_config_errors_helpfully() {
    let repo = TestRepo::new();
    let id = repo.add("t", Some("b"), Some("a"));
    let before = show_json(&repo, &id);

    let fake = FakeTmux::new();
    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", fake.bin_dir.display(), original_path);
    let out = Command::new(bin())
        .args(["start", &id, "--tmux"])
        .current_dir(&repo.dir)
        .env("PATH", new_path)
        .env_remove("TMUX")
        .env_remove("DOCKET_TERMINAL_CMD")
        .env_remove("TERM_PROGRAM")
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "expected failure when no spawn config is available; stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_lowercase();
    assert!(
        stderr.contains("docket_terminal_cmd"),
        "stderr should mention DOCKET_TERMINAL_CMD as the fix; got:\n{}",
        stderr
    );

    // status must NOT change — mark_in_progress runs only after delivery succeeds
    let after = show_json(&repo, &id);
    assert_eq!(
        before["status"], after["status"],
        "status must not change when spawn config is missing"
    );
    assert_eq!(
        before["updated_at"], after["updated_at"],
        "updated_at must not be bumped on failed spawn"
    );
}

#[cfg(unix)]
#[test]
fn start_tmux_inside_session_creates_new_session_and_switches_client() {
    let repo = TestRepo::new();
    let id = repo.add(
        "Make widgets",
        Some("widget body content"),
        Some("widgets exist when invoked"),
    );

    let fake = FakeTmux::new();
    let out = run_with_tmux(&repo, &["start", &id, "--tmux"], &fake);
    assert!(
        out.status.success(),
        "expected success; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let calls = fake.calls();
    assert!(!calls.is_empty(), "fake tmux should have been invoked at least once");

    // 1) new-session was called with a session name shaped `docket-T-N-<ts>`
    let new_session_call = calls
        .iter()
        .find(|c| c.first().map(|s| s.as_str()) == Some("new-session"))
        .unwrap_or_else(|| panic!("no `new-session` call recorded; calls:\n{:#?}", calls));
    let session_name_prefix = format!("docket-{}-", id);
    let has_session_name = new_session_call
        .windows(2)
        .any(|w| w[0] == "-s" && w[1].starts_with(&session_name_prefix));
    assert!(
        has_session_name,
        "new-session should be invoked with `-s {}<ts>`; got: {:?}",
        session_name_prefix, new_session_call
    );

    // 2) switch-client was called (because $TMUX is set)
    assert!(
        calls
            .iter()
            .any(|c| c.first().map(|s| s.as_str()) == Some("switch-client")),
        "switch-client should be called when $TMUX is set; got: {:#?}",
        calls
    );

    // 3) send-keys was called and references a tempfile path that exists with the prompt content
    let send_keys_call = calls
        .iter()
        .find(|c| c.first().map(|s| s.as_str()) == Some("send-keys"))
        .unwrap_or_else(|| panic!("no `send-keys` call recorded; calls:\n{:#?}", calls));
    // The shell command appears as one argv element among send-keys args.
    // It should reference an existing file path, and that file should contain the assembled prompt.
    let body_marker = "widget body content";
    let acceptance_marker = "widgets exist when invoked";

    // The send-keys argv itself must NOT contain the inline prompt body —
    // the prompt is delivered via tempfile, per the task spec.
    let send_keys_joined = send_keys_call.join(" ");
    assert!(
        !send_keys_joined.contains(body_marker),
        "send-keys argv should NOT contain inline prompt body; got: {:?}",
        send_keys_call
    );
    assert!(
        !send_keys_joined.contains(acceptance_marker),
        "send-keys argv should NOT contain inline acceptance text; got: {:?}",
        send_keys_call
    );

    // Find an arg that looks like a path that exists on disk; assert it carries the prompt.
    let prompt_file = send_keys_call
        .iter()
        .find_map(|arg| {
            for tok in arg.split_whitespace() {
                let p = PathBuf::from(tok);
                if p.is_file() {
                    return Some(p);
                }
            }
            None
        })
        .unwrap_or_else(|| {
            panic!(
                "send-keys should reference an existing tempfile path; got: {:?}",
                send_keys_call
            )
        });
    let prompt = fs::read_to_string(&prompt_file).expect("prompt tempfile should be readable");
    assert!(
        prompt.contains(body_marker),
        "tempfile should contain assembled prompt body; got:\n{}",
        prompt
    );
    assert!(
        prompt.contains(acceptance_marker),
        "tempfile should contain assembled prompt acceptance; got:\n{}",
        prompt
    );
    assert!(
        prompt.contains(&format!("# Task {}: Make widgets", id)),
        "tempfile should contain assembled prompt header; got:\n{}",
        prompt
    );

    // The send-keys command should invoke `claude` against the tempfile.
    assert!(
        send_keys_joined.contains("claude"),
        "send-keys should invoke claude; got: {:?}",
        send_keys_call
    );

    // 3) status transitioned to in_progress
    let after = show_json(&repo, &id);
    assert_eq!(
        after["status"], "in_progress",
        "status should flip to in_progress on tmux happy path"
    );
}

#[cfg(unix)]
#[test]
fn start_tmux_validation_errors_fire_before_any_tmux_invocation() {
    let repo = TestRepo::new();

    // Case A: missing id with --tmux should not touch tmux
    let fake_a = FakeTmux::new();
    let out_a = run_with_tmux(&repo, &["start", "T-99", "--tmux"], &fake_a);
    assert!(
        !out_a.status.success(),
        "missing id should fail; stdout: {}",
        String::from_utf8_lossy(&out_a.stdout)
    );
    assert!(
        fake_a.calls().is_empty(),
        "tmux must NOT be invoked when task id is missing; calls: {:#?}",
        fake_a.calls()
    );

    // Case B: done task with --tmux should not touch tmux
    let id = repo.add("t", Some("b"), Some("a"));
    let mark = repo.run(&["done", &id]);
    assert!(mark.status.success(), "done failed: {}", String::from_utf8_lossy(&mark.stderr));

    let fake_b = FakeTmux::new();
    let out_b = run_with_tmux(&repo, &["start", &id, "--tmux"], &fake_b);
    assert!(
        !out_b.status.success(),
        "done task should fail; stdout: {}",
        String::from_utf8_lossy(&out_b.stdout)
    );
    let stderr_b = String::from_utf8_lossy(&out_b.stderr);
    assert!(
        stderr_b.contains("docket status"),
        "stderr should hint at `docket status` to reopen; got:\n{}",
        stderr_b
    );
    assert!(
        fake_b.calls().is_empty(),
        "tmux must NOT be invoked when task is done; calls: {:#?}",
        fake_b.calls()
    );
}

#[cfg(unix)]
#[test]
fn start_without_tmux_flag_does_not_touch_tmux_even_inside_session() {
    let repo = TestRepo::new();
    let id = repo.add("widgets", Some("body"), Some("acceptance"));

    let fake = FakeTmux::new();
    // No --tmux flag, but TMUX is set and fake tmux is on PATH
    let out = run_with_tmux(&repo, &["start", &id], &fake);
    assert!(
        out.status.success(),
        "default start (no --tmux) should succeed; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("# Task"),
        "default start should print the assembled prompt to stdout; got:\n{}",
        stdout
    );
    assert!(
        fake.calls().is_empty(),
        "tmux must NOT be invoked without --tmux; calls: {:#?}",
        fake.calls()
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
