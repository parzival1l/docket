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
impl FakeTmux {
    /// Each logged line is "CALL\t<arg0>\t<arg1>\t..." — return one Vec per call.
    fn calls(&self) -> Vec<Vec<String>> {
        let raw = fs::read_to_string(&self.log_path).unwrap_or_default();
        raw.lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                if parts.next() != Some("CALL") {
                    return None;
                }
                Some(parts.map(String::from).collect())
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

/// `docket session reconcile`: place a fake transcript in `$HOME/.claude/projects/<cwd-encoded>/<uuid>.jsonl`
/// whose first `type:"user"` line starts with `# Task T-{N}:`, then assert the
/// placeholder row for T-N is rewritten to `<uuid>`.
#[test]
fn session_reconcile_rewrites_placeholder_to_matching_uuid() {
    let repo = TestRepo::new();
    let id = repo.add("widget");
    let n: i64 = id.trim_start_matches("T-").parse().unwrap();

    // Seed a stale placeholder row exactly like old `docket start --tmux` would have.
    let placeholder = format!("docket-T-{}-1779042263909646000", n);
    let out = repo.run(&["session", "link", &id, &placeholder]);
    assert!(out.status.success(), "link failed: {}", String::from_utf8_lossy(&out.stderr));

    // Fake $HOME/.claude/projects/<cwd-encoded>/<uuid>.jsonl with a matching header.
    let home = env::temp_dir().join(format!(
        "docket-sess-reconcile-home-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let encoded: String = repo
        .dir
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' { '-' } else { c })
        .collect();
    let proj_dir = home.join(".claude").join("projects").join(&encoded);
    fs::create_dir_all(&proj_dir).unwrap();
    let real_uuid = "abcdef01-2345-4678-9abc-def012345678";
    let transcript = proj_dir.join(format!("{}.jsonl", real_uuid));
    let body = format!(
        "{{\"type\":\"summary\",\"x\":1}}\n\
         {{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"# Task T-{}: widget\\n\\n## Body\\nb\\n\"}}}}\n",
        n
    );
    fs::write(&transcript, body).unwrap();

    let out = Command::new(bin())
        .args(["session", "reconcile"])
        .current_dir(&repo.dir)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "reconcile failed: stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(real_uuid) && stdout.contains(&placeholder),
        "reconcile should report the rewrite; got:\n{}",
        stdout
    );

    // The placeholder row should be gone, replaced by the real uuid.
    let j = repo.show_json(&id);
    let sessions: Vec<String> = j["agent_sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        !sessions.iter().any(|s| s == &placeholder),
        "placeholder should be removed; got: {:?}",
        sessions
    );
    assert!(
        sessions.iter().any(|s| s == real_uuid),
        "real uuid should be linked to the task; got: {:?}",
        sessions
    );

    let _ = fs::remove_dir_all(&home);
}

/// A docket-minted claude session id is a v4 UUID:
/// `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx` (36 chars, 4 dashes, hex).
#[cfg(unix)]
fn looks_like_v4_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        let want_dash = i == 8 || i == 13 || i == 18 || i == 23;
        if want_dash {
            if *b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    bytes[14] == b'4'
}

#[cfg(unix)]
#[test]
fn start_tmux_links_uuid_shaped_session_id_not_tmux_name() {
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
    let recorded = j["agent_sessions"][0].as_str().expect("session id present");
    assert!(
        looks_like_v4_uuid(recorded),
        "linked session id must be a v4 uuid (so it matches the claude transcript filename); got: {:?}",
        recorded
    );
    assert!(
        !recorded.starts_with("docket-T-"),
        "linked session id must NOT be the tmux placeholder name; got: {:?}",
        recorded
    );
}

#[cfg(unix)]
#[test]
fn start_tmux_send_keys_passes_session_id_uuid_to_claude() {
    let repo = TestRepo::new();
    let id = repo.add("widget");

    let fake = FakeTmux::new();
    let out = run_with_tmux(&repo, &["start", &id, "--tmux"], &fake);
    assert!(out.status.success());

    let calls = fake.calls();
    let send_keys = calls
        .iter()
        .find(|c| c.first().map(|s| s.as_str()) == Some("send-keys"))
        .expect("send-keys call expected");
    let joined = send_keys.join(" ");
    assert!(
        joined.contains("claude --session-id "),
        "send-keys command must invoke `claude --session-id <uuid>`; got: {:?}",
        send_keys
    );

    // Pull the uuid argument out of the shell command and confirm it matches
    // exactly what was linked.
    let uuid = joined
        .split_whitespace()
        .skip_while(|tok| *tok != "--session-id")
        .nth(1)
        .expect("--session-id should have an argument");
    assert!(
        looks_like_v4_uuid(uuid),
        "session id passed to claude must be a v4 uuid; got: {:?}",
        uuid
    );

    let j = repo.show_json(&id);
    let recorded = j["agent_sessions"][0].as_str().unwrap();
    assert_eq!(
        recorded, uuid,
        "linked session id must equal the one we handed to claude --session-id"
    );
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
