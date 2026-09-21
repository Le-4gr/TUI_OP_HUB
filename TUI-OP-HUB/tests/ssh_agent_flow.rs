//! BDD integration scenarios for the SSH key import → ssh-agent flow (D145).
//!
//! Feature: SSH keys reach the running ssh-agent automatically
//!   as a user I import several keys (some passphrase-protected)
//!   so that every terminal uses them without manual ssh-add
//!
//! These tests exercise the REAL pipeline with REAL ssh tools:
//! real `ssh-keygen` keys, a REAL `ssh-agent` on a private socket, the
//! exact storage path the TUI uses (`secrets::ssh_import::import_key_file`)
//! and the shared offer (`secrets::ssh_agent::offer_user_keys`) that backs
//! the TUI `S` flow, the headless startup offer and the
//! `POST /ssh/agent-offer` endpoint. Run with: `cargo test --test ssh_agent_flow`
//!
//! Rounds (the user asked: "import and check multiple times with ssh keys"):
//!   1. import 3 keys (1 passphrase-locked, pass typed) → offer → agent holds 3
//!   2. re-import the same keys → skipped, no duplicates in the agent
//!   3. delete the stored passphrase → offer skips the locked key
//!   4. wrong passphrase → rejected; correct one → unlocked again

use sqlx::SqlitePool;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tui_op_hub::db;
use tui_op_hub::repository;
use tui_op_hub::secrets;

/// Base64 of 32 'a' bytes — a valid 32-byte master key for tests.
const TEST_KEY: &str = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";
const PASS: &str = "mydesk-D145-test-42";

async fn given_fresh_database() -> Arc<SqlitePool> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::run_migrations(&pool).await.unwrap();
    Arc::new(pool)
}

fn ssh_tools_available() -> bool {
    ["ssh-keygen", "ssh-add", "ssh-agent"]
        .iter()
        .all(|b| Command::new(b).arg("-V").output().is_ok()
            || Command::new(b).output().is_ok()
            || which_works(b))
}

fn which_works(bin: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null", bin))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Generate a real ed25519 key; returns (private_path, public_path).
fn given_ssh_key(dir: &std::path::Path, name: &str, passphrase: Option<&str>) -> (PathBuf, PathBuf) {
    let priv_path = dir.join(name);
    let pub_path = dir.join(format!("{}.pub", name));
    let out = Command::new("ssh-keygen")
        .args(["-t", "ed25519"])
        .args(["-f", priv_path.to_str().unwrap()])
        .args(["-N", passphrase.unwrap_or("")])
        .args(["-C", format!("{}@mydesk-test", name).as_str()])
        .stdin(Stdio::null())
        .output()
        .expect("ssh-keygen spawn");
    assert!(
        out.status.success(),
        "ssh-keygen failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(std::fs::metadata(&priv_path).is_ok());
    assert!(std::fs::metadata(&pub_path).is_ok());
    (priv_path, pub_path)
}

/// Start a REAL ssh-agent on a private unix socket and point this test
/// process at it. Returns the agent pid for cleanup. `ssh-agent -a` forks
/// the daemon and the parent exits, so output redirection goes to FILES
/// (never pipes — the daemon would hold the pipe open and hang us).
fn given_running_agent(dir: &std::path::Path) -> u32 {
    let sock = dir.join("agent.sock");
    let env_file = dir.join("agent-env.txt");
    let _ = std::fs::remove_file(&sock);
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ssh-agent -a '{}' > '{}' 2>&1",
            sock.display(),
            env_file.display()
        ))
        .status()
        .expect("ssh-agent spawn");
    assert!(status.success(), "ssh-agent did not start");
    // wait for the socket to appear (max 3s)
    let mut appeared = false;
    for _ in 0..60 {
        if std::fs::metadata(&sock).is_ok() {
            appeared = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(appeared, "ssh-agent socket never appeared");

    // point THIS process at the agent (ensure_agent() will then use it)
    std::env::set_var("SSH_AUTH_SOCK", sock.display().to_string());
    // parse the pid from the env dump for cleanup
    let env_text = std::fs::read_to_string(&env_file).unwrap_or_default();
    let mut pid = String::new();
    for line in env_text.lines() {
        if let Some(rest) = line.strip_prefix("SSH_AGENT_PID=") {
            pid = rest.split(';').next().unwrap_or("").to_string();
        }
    }
    pid.parse::<u32>().unwrap_or(0)
}

/// How many keys does the agent currently hold?
fn agent_key_count() -> (i32, String) {
    let out = Command::new("ssh-add").arg("-l").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    (out.status.code().unwrap_or(-1), text)
}

#[tokio::test]
async fn given_three_keys_when_import_and_offer_repeatedly_then_agent_exact_and_idempotent() {
    if !ssh_tools_available() {
        eprintln!("ssh tools missing — scenario skipped (CI fallback)");
        return;
    }
    std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
    let dir = std::env::temp_dir().join(format!("mydesk-ssh-flow-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let agent_pid = given_running_agent(&dir);

    let pool = given_fresh_database().await;
    // secrets.user_id references user_profiles.id → create the profile
    let uid = repository::get_or_create_user(&pool, "test-user").await.unwrap().id;
    let (plain_a, plain_a_pub) = given_ssh_key(&dir, "plain_a", None);
    let (locked, locked_pub) = given_ssh_key(&dir, "locked", Some(PASS));
    let (plain_b, plain_b_pub) = given_ssh_key(&dir, "plain_b", None);

    // ── ROUND 1: import 3 keys (typed passphrase for the locked one) → offer
    let r = secrets::ssh_import::import_key_file(&pool, &uid, &plain_a, Some(&plain_a_pub), None)
        .await
        .unwrap();
    assert!(r, "plain_a should import");
    let r = secrets::ssh_import::import_key_file(&pool, &uid, &locked, Some(&locked_pub), Some(PASS))
        .await
        .unwrap();
    assert!(r, "locked should import");
    let r = secrets::ssh_import::import_key_file(&pool, &uid, &plain_b, Some(&plain_b_pub), None)
        .await
        .unwrap();
    assert!(r, "plain_b should import");

    let all = repository::list_secrets(&pool, &uid).await.unwrap();
    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    for want in ["ssh:plain_a", "ssh:locked", "ssh:plain_b"] {
        assert!(names.contains(&want), "missing secret {}", want);
    }
    assert!(
        names.contains(&"ssh-pass:locked"),
        "the typed passphrase must be persisted as ssh-pass:locked"
    );
    assert!(names.contains(&"ssh-pub:plain_a"), "public half stored");
    let out = secrets::ssh_agent::offer_user_keys(&pool, &uid).await;
    eprintln!("round-1 offer: offered={} unlocked={} skipped={}", out.offered, out.unlocked, out.skipped);
    assert_eq!(
        (out.offered, out.unlocked, out.skipped),
        (2, 1, 0),
        "2 plain offered + 1 locked unlocked silently"
    );
    let (code, list) = agent_key_count();
    assert_eq!(code, 0, "agent lists keys");
    assert_eq!(
        list.lines().filter(|l| !l.trim().is_empty()).count(),
        3,
        "agent must hold exactly the 3 imported keys"
    );
    assert!(list.contains("plain_a@mydesk-test"));
    assert!(list.contains("locked@mydesk-test"));
    assert!(list.contains("plain_b@mydesk-test"));

    // ── ROUND 2: re-import the SAME keys → skipped, no duplicates
    for priv_path in [&plain_a, &locked, &plain_b] {
        let stem = priv_path.file_name().unwrap().to_str().unwrap();
        let pub_path = dir.join(format!("{}.pub", stem));
        let r = secrets::ssh_import::import_key_file(&pool, &uid, priv_path, Some(&pub_path), None)
            .await
            .unwrap();
        assert!(!r, "duplicate import must be skipped");
    }
    let all2 = repository::list_secrets(&pool, &uid).await.unwrap();
    assert_eq!(all2.len(), all.len(), "no duplicate secrets created");
    let out2 = secrets::ssh_agent::offer_user_keys(&pool, &uid).await;
    let (code2, list2) = agent_key_count();
    assert_eq!(code2, 0);
    assert_eq!(
        list2.lines().filter(|l| !l.trim().is_empty()).count(),
        3,
        "agent stays at exactly 3 keys (idempotent offers)"
    );
    let _ = out2; // counts may vary by ssh-add duplicate semantics — the LIST is the contract

    // ── ROUND 3: stored passphrase deleted → locked key is skipped, not crashed
    let all3 = repository::list_secrets(&pool, &uid).await.unwrap();
    for p in all3.iter().filter(|s| s.name == "ssh-pass:locked") {
        repository::delete_secret(&pool, &p.id).await.unwrap();
    }
    let out3 = secrets::ssh_agent::offer_user_keys(&pool, &uid).await;
    assert_eq!(
        (out3.offered, out3.unlocked, out3.skipped),
        (2, 0, 1),
        "locked key without stored passphrase is SKIPPED (not added, not crashed)"
    );
    let (code3, list3) = agent_key_count();
    assert_eq!(code3, 0);
    assert_eq!(
        list3.lines().filter(|l| !l.trim().is_empty()).count(),
        3,
        "agent keeps the already-unlocked key until the agent dies"
    );

    // ── ROUND 4: wrong passphrase rejected, correct one unlocks again
    let pem = std::fs::read_to_string(&locked).unwrap();
    let wrong = secrets::ssh_agent::add_key_with_pass("ssh:locked", &pem, "wrong-passphrase");
    assert!(wrong.is_err(), "a wrong passphrase must be rejected");
    let pass_enc = secrets::encrypt_for_user(&pool, &uid, PASS).await.unwrap();
    let meta = repository::SecretMeta {
        secret_group: Some("ssh".to_string()),
        username: None,
        url: None,
        email: None,
        passphrase_protected: false,
        ssh_agent: false,
    };
    repository::create_secret_meta(&pool, &uid, "ssh-pass:locked", &pass_enc, "password", false, &meta)
        .await
        .unwrap();
    let out4 = secrets::ssh_agent::offer_user_keys(&pool, &uid).await;
    assert_eq!(
        (out4.offered, out4.unlocked, out4.skipped),
        (2, 1, 0),
        "with the passphrase re-stored the locked key unlocks again"
    );
    // cleanup: kill the agent, remove the temp dir
    if agent_pid > 0 {
        let _ = Command::new("kill")
            .arg(agent_pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn given_locked_key_when_imported_without_pass_then_no_pass_secret_is_created() {
    std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
    let pool = given_fresh_database().await;
    let uid = repository::get_or_create_user(&pool, "test-user-nopass")
        .await
        .unwrap()
        .id;
    let dir = std::env::temp_dir().join(format!("mydesk-ssh-nopass-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (locked, _pub) = given_ssh_key(&dir, "locked_nopass", Some(PASS));
    // no passphrase typed at import (the user pressed I without `p` first)
    let r = secrets::ssh_import::import_key_file(&pool, &uid, &locked, None, None)
        .await
        .unwrap();
    assert!(r);
    let all = repository::list_secrets(&pool, &uid).await.unwrap();
    assert!(all.iter().any(|s| s.name == "ssh:locked_nopass"));
    assert!(
        !all.iter().any(|s| s.name == "ssh-pass:locked_nopass"),
        "no passphrase was typed → no ssh-pass secret must exist"
    );
    let locked_secret = all.iter().find(|s| s.name == "ssh:locked_nopass").unwrap();
    assert!(
        locked_secret.passphrase_protected,
        "the stored key keeps its passphrase_protected flag (terminal unlock path)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
