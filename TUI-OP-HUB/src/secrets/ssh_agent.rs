//! ssh-agent integration (US-SEC): offer stored SSH keys to the agent on
//! login/startup, and open SSH terminals using a stored key.

use crate::error::{AppError, AppResult};

/// Outcome of a connection test (US-SSH-05).
#[derive(Debug, Clone)]
pub struct ConnectionTest {
    pub ok: bool,
    pub detail: String,
}

/// Build the non-interactive ssh command used to TEST a host (US-SSH-05):
/// BatchMode (no password prompt), bounded ConnectTimeout and relaxed host
/// key checking (reachability is what's being verified). Public for tests.
pub fn ssh_test_command(
    hostname: &str,
    port: i32,
    username: Option<&str>,
    key_path: Option<&str>,
) -> std::process::Command {
    let target = match username {
        Some(u) if !u.is_empty() => format!("{}@{}", u, hostname),
        _ => hostname.to_string(),
    };
    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "StrictHostKeyChecking=no",
        "-p",
        &port.to_string(),
    ]);
    if let Some(key) = key_path {
        if !key.is_empty() {
            cmd.args(["-i", key]);
        }
    }
    cmd.args([target, "exit".to_string()]);
    cmd
}

/// Run the connection test synchronously (call from spawn_blocking).
pub fn run_connection_test(
    hostname: &str,
    port: i32,
    username: Option<&str>,
    key_path: Option<&str>,
) -> ConnectionTest {
    if !crate::keygen::which("ssh") {
        return ConnectionTest {
            ok: false,
            detail: "ssh binary not available".to_string(),
        };
    }
    match ssh_test_command(hostname, port, username, key_path).output() {
        Ok(out) => {
            if out.status.success() {
                ConnectionTest {
                    ok: true,
                    detail: "connection OK".to_string(),
                }
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let first = stderr
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("connection failed");
                ConnectionTest {
                    ok: false,
                    detail: first.chars().take(120).collect(),
                }
            }
        }
        Err(e) => ConnectionTest {
            ok: false,
            detail: format!("{}", e),
        },
    }
}

/// Ensure an ssh-agent is reachable: if `SSH_AUTH_SOCK` is already set the
/// running agent is used; otherwise a new agent is started and its
/// environment is applied to this process so spawned shells inherit it.
pub fn ensure_agent() -> AppResult<()> {
    if std::env::var("SSH_AUTH_SOCK")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return Ok(()); // an agent is already configured
    }
    if !crate::keygen::which("ssh-agent") {
        return Err(AppError::Other("ssh-agent not installed".into()));
    }
    let out = std::process::Command::new("ssh-agent")
        .arg("-s")
        .output()
        .map_err(|e| AppError::Io(e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut sock = None;
    let mut pid = None;
    for line in text.lines() {
        // lines look like: SSH_AUTH_SOCK=/tmp/ssh-XXXX/agent.1234; export SSH_AUTH_SOCK;
        if let Some(rest) = line.strip_prefix("SSH_AUTH_SOCK=") {
            if let Some(v) = rest.split(';').next() {
                sock = Some(v.to_string());
            }
        }
        if let Some(rest) = line.strip_prefix("SSH_AGENT_PID=") {
            if let Some(v) = rest.split(';').next() {
                pid = Some(v.to_string());
            }
        }
    }
    let Some(sock) = sock else {
        return Err(AppError::Other("ssh-agent produced no socket".into()));
    };
    std::env::set_var("SSH_AUTH_SOCK", &sock);
    if let Some(pid) = pid {
        std::env::set_var("SSH_AGENT_PID", pid);
    }
    tracing::info!(sock = %sock, "started ssh-agent");
    Ok(())
}

/// Offer a private key (PEM content) to the running ssh-agent. The key is
/// written to a 0600 temp file, added, then removed immediately.
pub fn add_key_to_agent(name: &str, pem: &str) -> AppResult<()> {
    if !crate::keygen::which("ssh-add") {
        return Err(AppError::Other("ssh-add not installed".into()));
    }
    ensure_agent()?;
    let dir = std::env::temp_dir().join(format!("tui-op-hub-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e))?;
    let file = dir.join(format!("key-{}", name.replace('/', "_").replace(" ", "_")));
    std::fs::write(&file, pem).map_err(|e| AppError::Io(e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
    }
    let out = std::process::Command::new("ssh-add")
        .arg(&file)
        .output()
        .map_err(|e| AppError::Io(e))?;
    let _ = std::fs::remove_file(&file);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::Other(format!(
            "ssh-add failed for '{}': {}",
            name,
            err.trim()
        )));
    }
    tracing::info!(key = %name, "added key to ssh-agent");
    Ok(())
}

/// D145: offer a PASSPHRASE-protected private key to the agent
/// non-interactively. ssh-add runs with SSH_ASKPASS_REQUIRE=force
/// (OpenSSH >= 8.4) and an askpass script that echoes the passphrase from
/// an env var — no secret ever touches disk in plaintext. Older OpenSSH
/// ignores the REQUIRE variable and falls back to the DISPLAY + no-tty
/// path, which our piped stdio satisfies (DISPLAY defaults to :0).
pub fn add_key_with_pass(name: &str, pem: &str, passphrase: &str) -> AppResult<()> {
    if !crate::keygen::which("ssh-add") {
        return Err(AppError::Other("ssh-add not installed".into()));
    }
    ensure_agent()?;
    let dir = std::env::temp_dir().join(format!("tui-op-hub-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e))?;
    let file = dir.join(format!("key-{}", name.replace('/', "_").replace(" ", "_")));
    std::fs::write(&file, pem).map_err(|e| AppError::Io(e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
    }
    let script = dir.join(format!(
        "askpass-{}",
        name.replace('/', "_").replace(" ", "_")
    ));
    // The script contains NO secret — it echoes the env var passed below.
    std::fs::write(
        &script,
        "#!/bin/sh\necho \"$MYDESK_ASKPASS\"\n",
    )
    .map_err(|e| AppError::Io(e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut child = std::process::Command::new("ssh-add")
        .arg(&file)
        .env("SSH_ASKPASS", &script)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(
            "DISPLAY",
            std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string()),
        )
        .env("MYDESK_ASKPASS", passphrase)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Io(e))?;
    // ssh-add LOOPS forever when the askpass keeps returning a wrong
    // passphrase (it re-prompts instead of failing) — bound the wait and
    // kill it, reporting a wrong-passphrase-style error instead.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(AppError::Io(e)),
        }
    };
    let mut stderr_text = String::new();
    if let Some(mut p) = child.stderr.take() {
        use std::io::Read;
        let _ = p.read_to_string(&mut stderr_text);
    }
    // cleanup AFTER the child exited (removing the askpass script early
    // raced ssh-add's prompt and broke the unlock)
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(&script);
    let ok = match &status {
        Some(st) => st.success() || stderr_text.contains("already in agent"),
        None => false,
    };
    if !ok {
        let reason = match status {
            None => {
                if stderr_text.contains("incorrect passphrase") {
                    "incorrect passphrase (retry loop killed)".to_string()
                } else {
                    "timed out — is ssh-agent reachable?".to_string()
                }
            }
            Some(_) => stderr_text.trim().to_string(),
        };
        return Err(AppError::Other(format!(
            "ssh-add failed for '{}': {}",
            name, reason
        )));
    }
    tracing::info!(key = %name, "unlocked passphrase key into ssh-agent");
    Ok(())
}

/// D145: outcome of an agent offer run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct OfferOutcome {
    /// Plain keys added to the agent.
    pub offered: usize,
    /// Passphrase-locked keys unlocked non-interactively (stored pass).
    pub unlocked: usize,
    /// Keys that could not be added (no agent, no stored passphrase, …).
    pub skipped: usize,
}

/// D145: offer every `ssh_key` secret of a user to the running agent.
/// Shared by the TUI `S` flow, the headless startup offer and the
/// `/ssh/agent-offer` endpoint. Passphrase-locked keys are unlocked when
/// the import stored the passphrase as `ssh-pass:<stem>`; locked keys
/// WITHOUT a stored passphrase are skipped here (the TUI opens a terminal
/// for those; headless paths cannot prompt).
pub async fn offer_user_keys(pool: &sqlx::SqlitePool, user_id: &str) -> OfferOutcome {
    let mut out = OfferOutcome::default();
    let Ok(all) = crate::repository::list_secrets(pool, user_id).await else {
        return out;
    };
    for secret in all
        .iter()
        .filter(|s| s.ssh_agent && s.secret_kind == "ssh_key")
    {
        match crate::secrets::decrypt_for_user(pool, user_id, &secret.value_enc).await {
            Ok(pem) => {
                if secret.passphrase_protected {
                    let stem = secret.name.strip_prefix("ssh:").unwrap_or(&secret.name);
                    let pass_name = format!("ssh-pass:{}", stem);
                    let mut unlocked_now = false;
                    if let Some(p) = all.iter().find(|s| s.name == pass_name) {
                        if let Ok(pass) =
                            crate::secrets::decrypt_for_user(pool, user_id, &p.value_enc).await
                        {
                            unlocked_now = add_key_with_pass(&secret.name, &pem, &pass).is_ok();
                        }
                    }
                    if unlocked_now {
                        out.unlocked += 1;
                    } else {
                        out.skipped += 1;
                    }
                } else if add_key_to_agent(&secret.name, &pem).is_ok() {
                    out.offered += 1;
                } else {
                    out.skipped += 1;
                }
            }
            Err(_) => out.skipped += 1,
        }
    }
    tracing::info!(
        offered = out.offered,
        unlocked = out.unlocked,
        skipped = out.skipped,
        "ssh-agent offer complete"
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_key_rejects_missing_agent_keyfile() {
        // A non-existent key path must produce an error, not a panic.
        let r = add_key_to_agent("nonexistent key", "not a pem");
        // Either ssh-add rejects the garbage key or the agent is missing:
        // both are errors; success is only possible when ssh-add exists AND
        // accepts the content (it does not).
        if crate::keygen::which("ssh-add") {
            assert!(r.is_err());
        }
    }

    #[test]
    fn given_host_when_test_command_built_then_flags_and_target_correct() {
        // US-SSH-05: non-interactive probe with bounded timeout
        let cmd = ssh_test_command(
            "prod.example.com",
            2222,
            Some("root"),
            Some("~/.ssh/id_ed25519"),
        );
        let program = format!("{:?}", cmd);
        assert!(program.contains("ssh"));
        assert!(program.contains("BatchMode=yes"));
        assert!(program.contains("ConnectTimeout=5"));
        assert!(program.contains("StrictHostKeyChecking=no"));
        assert!(program.contains("2222"));
        assert!(program.contains("~/.ssh/id_ed25519"));
        assert!(program.contains("root@prod.example.com"));
        assert!(program.contains("exit"));

        // No username → bare hostname target; no key → no -i flag
        let cmd = ssh_test_command("host", 22, None, None);
        let program = format!("{:?}", cmd);
        assert!(program.contains("\"host\""));
        assert!(!program.contains("-i"));
    }

    #[test]
    fn given_unreachable_host_when_tested_then_reports_failure() {
        // Port 1 on localhost: connection refused immediately (hermetic)
        let result = run_connection_test("127.0.0.1", 1, None, None);
        assert!(!result.ok, "closed port must fail the test");
        assert!(!result.detail.is_empty());
    }
}
