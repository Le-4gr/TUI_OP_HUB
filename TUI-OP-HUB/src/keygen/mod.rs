//! SSH & GPG key generation for the Secrets tab (US-SEC-01).
//!
//! Uses the well-known system tools (`ssh-keygen`, `gpg`) instead of
//! reimplementing crypto. The private key path and its passphrase are stored
//! as secrets (`ssh_key` / `gpg_key` kinds flagged `requires_reauth`), so
//! using them requires more than just being logged in.

use crate::error::{AppError, AppResult};
use std::path::Path;

/// Result of a key generation run.
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedKey {
    /// `ssh_key` or `gpg_key`
    pub kind: String,
    pub name: String,
    /// Path to the private key (or `gpg:<email>` for GPG keys)
    pub private_path: String,
    pub passphrase: String,
}

/// True when `program` exists on `PATH` (pure helper, testable).
pub fn which(program: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        return path
            .split(':')
            .any(|dir| Path::new(dir).join(program).is_file());
    }
    false
}

/// Pick the first available interactive process viewer (US-PROC). Users already
/// know `btop`/`htop`/`top` — launch those instead of building something new.
pub fn pick_process_viewer() -> Option<&'static str> {
    ["btop", "htop", "top"].into_iter().find(|p| which(p))
}

/// Generate an ed25519 SSH keypair using `ssh-keygen` (US-SEC-01).
///
/// The private key is written to `<dir>/<name>_ed25519` with ssh-keygen's
/// restrictive default permissions; the passphrase is returned so it can be
/// stored as an encrypted secret.
pub fn generate_ssh_key(dir: &Path, name: &str, passphrase: &str) -> AppResult<GeneratedKey> {
    if !which("ssh-keygen") {
        return Err(AppError::Other("ssh-keygen not found on PATH".into()));
    }
    std::fs::create_dir_all(dir).map_err(|e| AppError::Other(format!("mkdir failed: {e}")))?;
    let private_path = dir.join(format!("{name}_ed25519"));
    if private_path.exists() {
        return Err(AppError::Other(format!(
            "ssh key '{name}' already exists at {}",
            private_path.display()
        )));
    }
    let output = std::process::Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-N",
            passphrase,
            "-C",
            &format!("tui-op-hub:{name}"),
            "-f",
        ])
        .arg(&private_path)
        .output()
        .map_err(|e| AppError::Other(format!("ssh-keygen failed: {e}")))?;
    if !output.status.success() {
        return Err(AppError::Other("ssh-keygen failed".into()));
    }
    Ok(GeneratedKey {
        kind: "ssh_key".to_string(),
        name: name.to_string(),
        private_path: private_path.display().to_string(),
        passphrase: passphrase.to_string(),
    })
}

/// Generate a GPG key with `gpg --batch --quick-generate-key` (US-SEC-01).
pub fn generate_gpg_key(real_name: &str, email: &str, passphrase: &str) -> AppResult<GeneratedKey> {
    if !which("gpg") {
        return Err(AppError::Other("gpg not found on PATH".into()));
    }
    let uid = format!("{real_name} <{email}>");
    let mut cmd = std::process::Command::new("gpg");
    cmd.arg("--batch");
    if !passphrase.is_empty() {
        cmd.args(["--pinentry-mode", "loopback", "--passphrase", passphrase]);
    }
    let status = cmd
        .args(["--quick-generate-key", &uid, "ed25519", "sign", "never"])
        .output()
        .map_err(|e| AppError::Other(format!("gpg failed: {e}")))?;
    if !status.status.success() {
        return Err(AppError::Other("gpg key generation failed".into()));
    }
    Ok(GeneratedKey {
        kind: "gpg_key".to_string(),
        name: real_name.to_string(),
        private_path: format!("gpg:{email}"),
        passphrase: passphrase.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_system_tools() {
        // These exist on any normal Linux CI/host; `definitely-missing` does not
        assert!(which("sh"));
        assert!(!which("definitely-not-a-real-tool-xyz"));
    }

    #[test]
    fn process_viewer_prefers_known_tools() {
        // top is POSIX-mandatory; if anything is found it must be one of these
        if let Some(viewer) = pick_process_viewer() {
            assert!(["btop", "htop", "top"].contains(&viewer));
        }
    }

    #[test]
    fn ssh_keygen_generates_keypair() {
        if !which("ssh-keygen") {
            return; // environment without openssh — skip
        }
        let dir = std::env::temp_dir().join(format!("tui-op-hub-keygen-{}", uuid::Uuid::new_v4()));
        let key = generate_ssh_key(&dir, "deploy", "s3cret").unwrap();
        assert_eq!(key.kind, "ssh_key");
        assert_eq!(key.passphrase, "s3cret");
        assert!(Path::new(&key.private_path).exists(), "private key written");
        let pub_path = format!("{}.pub", key.private_path);
        assert!(Path::new(&pub_path).exists(), "public key written");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ssh_keygen_rejects_existing_path() {
        if !which("ssh-keygen") {
            return;
        }
        let dir = std::env::temp_dir().join(format!("tui-op-hub-keygen2-{}", uuid::Uuid::new_v4()));
        let _ = generate_ssh_key(&dir, "dup", "");
        assert!(
            generate_ssh_key(&dir, "dup", "").is_err(),
            "must not overwrite"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
