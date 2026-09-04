//! ssh-agent integration (US-SEC): offer stored SSH keys to the agent on
//! login/startup, and open SSH terminals using a stored key.

use crate::error::{AppError, AppResult};

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
}
