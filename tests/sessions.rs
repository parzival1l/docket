//! End-to-end tests for the agent_sessions / session<->task two-way link.
//!
//! Each test runs in its own tempdir-backed `.docket/` for parallel safety.

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
            "docket-sess-test-{}-{}-{}",
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

    fn add(&self, title: &str) -> String {
        let out = self.run(&["add", title, "--body", "b", "--acceptance", "a"]);
        assert!(
            out.status.success(),
            "docket add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        stdout.split_whitespace().next().unwrap().to_string()
    }

    fn show_json(&self, id: &str) -> serde_json::Value {
        let out = self.run(&["show", id, "--json"]);
        assert!(
            out.status.success(),
            "show --json failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("show --json produced invalid JSON")
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn show_json_includes_empty_agent_sessions_by_default() {
    let repo = TestRepo::new();
    let id = repo.add("widget");
    let j = repo.show_json(&id);
    let sessions = j.get("agent_sessions").unwrap_or_else(|| {
        panic!("show --json must expose `agent_sessions` field; got: {}", j)
    });
    assert!(
        sessions.is_array(),
        "agent_sessions must be an array; got: {}",
        sessions
    );
    assert_eq!(
        sessions.as_array().unwrap().len(),
        0,
        "a task with no linked sessions should have an empty agent_sessions list; got: {}",
        sessions
    );
}

#[test]
fn session_link_then_show_lists_session_on_task() {
    let repo = TestRepo::new();
    let id = repo.add("widget");

    let out = repo.run(&["session", "link", &id, "sess-abc"]);
    assert!(
        out.status.success(),
        "session link failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let j = repo.show_json(&id);
    let sessions: Vec<String> = j["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(sessions, vec!["sess-abc".to_string()]);
}

#[test]
fn session_show_resolves_back_to_task() {
    let repo = TestRepo::new();
    let id = repo.add("widget");
    let out = repo.run(&["session", "link", &id, "sess-abc"]);
    assert!(out.status.success());

    let show = repo.run(&["session", "show", "sess-abc", "--json"]);
    assert!(
        show.status.success(),
        "session show failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let j: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(
        j["task"].as_str().unwrap(),
        id,
        "session show must resolve back to the linked task id; got: {}",
        j
    );
}

#[test]
fn task_can_hold_multiple_distinct_sessions() {
    let repo = TestRepo::new();
    let id = repo.add("widget");
    assert!(repo.run(&["session", "link", &id, "sess-a"]).status.success());
    assert!(repo.run(&["session", "link", &id, "sess-b"]).status.success());
    assert!(repo.run(&["session", "link", &id, "sess-c"]).status.success());

    let j = repo.show_json(&id);
    let mut sessions: Vec<String> = j["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    sessions.sort();
    assert_eq!(
        sessions,
        vec!["sess-a".to_string(), "sess-b".to_string(), "sess-c".to_string()]
    );
}

#[test]
fn linking_already_attached_session_moves_rather_than_duplicates() {
    let repo = TestRepo::new();
    let id_one = repo.add("first");
    let id_two = repo.add("second");

    assert!(repo
        .run(&["session", "link", &id_one, "sess-x"])
        .status
        .success());

    // Move to the second task. Same session id, different task.
    let mv = repo.run(&["session", "link", &id_two, "sess-x"]);
    assert!(
        mv.status.success(),
        "re-linking an attached session must succeed (move semantics); got: {}",
        String::from_utf8_lossy(&mv.stderr)
    );

    // First task no longer carries it.
    let j1 = repo.show_json(&id_one);
    let s1: Vec<&str> = j1["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        s1.is_empty(),
        "old task should no longer list moved session; got: {:?}",
        s1
    );

    // Second task carries it (exactly once).
    let j2 = repo.show_json(&id_two);
    let s2: Vec<&str> = j2["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(s2, vec!["sess-x"]);

    // session show resolves to the new task.
    let show = repo.run(&["session", "show", "sess-x", "--json"]);
    assert!(show.status.success());
    let j: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(j["task"].as_str().unwrap(), id_two);
}

#[test]
fn unlinking_session_clears_both_sides_atomically() {
    let repo = TestRepo::new();
    let id = repo.add("widget");
    assert!(repo
        .run(&["session", "link", &id, "sess-y"])
        .status
        .success());

    let un = repo.run(&["session", "unlink", "sess-y"]);
    assert!(
        un.status.success(),
        "session unlink failed: {}",
        String::from_utf8_lossy(&un.stderr)
    );

    // Task side: agent_sessions no longer contains the id.
    let j = repo.show_json(&id);
    let s: Vec<&str> = j["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        s.is_empty(),
        "task agent_sessions should be empty after unlink; got: {:?}",
        s
    );

    // Session side: resolving fails or returns no-task — no dangling pointer.
    let show = repo.run(&["session", "show", "sess-y", "--json"]);
    if show.status.success() {
        let j: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
        assert!(
            j.get("task").map(|v| v.is_null()).unwrap_or(true),
            "session show of unlinked session must NOT point at a task; got: {}",
            j
        );
    }
}

#[test]
fn two_way_invariant_holds_for_every_linked_pair() {
    // For every (session_id, task_id) currently linked, both views agree:
    //   task.agent_sessions contains session_id  AND
    //   session.task == task_id
    let repo = TestRepo::new();
    let a = repo.add("a");
    let b = repo.add("b");
    assert!(repo.run(&["session", "link", &a, "s1"]).status.success());
    assert!(repo.run(&["session", "link", &a, "s2"]).status.success());
    assert!(repo.run(&["session", "link", &b, "s3"]).status.success());

    let pairs = [(&a, "s1"), (&a, "s2"), (&b, "s3")];
    for (task_id, sess) in pairs.iter() {
        let task_json = repo.show_json(task_id);
        let task_sessions: Vec<&str> = task_json["agent_sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            task_sessions.contains(sess),
            "task {} should list session {}; got: {:?}",
            task_id,
            sess,
            task_sessions
        );

        let show = repo.run(&["session", "show", sess, "--json"]);
        assert!(show.status.success(), "session show {} failed", sess);
        let sj: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
        assert_eq!(
            sj["task"].as_str().unwrap(),
            task_id.as_str(),
            "session {} should resolve back to task {}; got: {}",
            sess,
            task_id,
            sj
        );
    }
}

// -----------------------------------------------------------------------------
// `docket start --tmux` records the spawned session id automatically.
// -----------------------------------------------------------------------------

#[cfg(unix)]
struct FakeTmux {
    bin_dir: PathBuf,
    #[allow(dead_code)]
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
            "docket-sess-faketmux-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        fs::create_dir_all(&bin_dir).unwrap();
        let log_path = bin_dir.join("calls.log");
        let script_path = bin_dir.join("tmux");
        let script = format!(
            "#!/bin/sh\n\
LOG='{log}'\n\
{{ printf 'CALL'; for a in \"$@\"; do printf '\\t%s' \"$a\"; done; printf '\\n'; }} >> \"$LOG\"\n\
case \"$1\" in\n\
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

#[cfg(unix)]
#[test]
fn start_tmux_appends_session_to_task_agent_sessions() {
    let repo = TestRepo::new();
    let id = repo.add("widget");

    let fake = FakeTmux::new();
    let out = run_with_tmux(&repo, &["start", &id, "--tmux"], &fake);
    assert!(
        out.status.success(),
        "start --tmux failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let j = repo.show_json(&id);
    let sessions: Vec<String> = j["agent_sessions"]
        .as_array()
        .unwrap_or_else(|| panic!("agent_sessions must be an array; got: {}", j))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        sessions.len(),
        1,
        "expected exactly one session appended by --tmux; got: {:?}",
        sessions
    );

    // The recorded id resolves back to this task.
    let recorded = &sessions[0];
    let show = repo.run(&["session", "show", recorded, "--json"]);
    assert!(show.status.success());
    let sj: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(sj["task"].as_str().unwrap(), id);
}

#[cfg(unix)]
#[test]
fn start_tmux_twice_appends_distinct_sessions() {
    let repo = TestRepo::new();
    let id = repo.add("widget");

    let fake = FakeTmux::new();
    assert!(run_with_tmux(&repo, &["start", &id, "--tmux"], &fake)
        .status
        .success());
    // ensure timestamp resolution moves
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(run_with_tmux(&repo, &["start", &id, "--tmux"], &fake)
        .status
        .success());

    let j = repo.show_json(&id);
    let sessions: Vec<String> = j["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        sessions.len(),
        2,
        "two --tmux invocations should append two distinct session ids; got: {:?}",
        sessions
    );
    assert_ne!(
        sessions[0], sessions[1],
        "the two appended ids must differ"
    );
}
