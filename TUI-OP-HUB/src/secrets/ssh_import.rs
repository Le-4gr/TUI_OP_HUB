//! D145: SSH key file import (the STORAGE half) — shared by the TUI import
//! window and the integration tests, so tests exercise the exact production
//! path. The agent half (ssh-add) lives in `ssh_agent`.

use crate::repository::{self, SecretMeta};
use crate::secrets;
use sqlx::SqlitePool;
use std::path::Path;

/// D145: true when a private key FILE is passphrase-protected. Covers
/// legacy PEM keys (`Proc-Type: 4,ENCRYPTED` / `BEGIN .. ENCRYPTED`) AND
/// the modern OpenSSH format — which never contains the literal text
/// "ENCRYPTED"; there the cipher name inside the key blob is checked
/// (`none` = unencrypted, e.g. `aes256-ctr` = locked).
pub fn is_passphrase_protected(pem: &str) -> bool {
    if pem.contains("ENCRYPTED") {
        return true; // legacy PEM
    }
    // OpenSSH format: base64 blob between the BEGIN/END OPENSSH markers
    let blob: String = pem
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.starts_with("-----") && !l.starts_with("Comment:") && !l.is_empty())
        .collect();
    use base64::engine::general_purpose;
    use base64::Engine as _;
    let Ok(bytes) = general_purpose::STANDARD.decode(blob) else {
        return false;
    };
    const MAGIC: &[u8] = b"openssh-key-v1\x00";
    if bytes.len() < MAGIC.len() + 4 || &bytes[..MAGIC.len()] != MAGIC {
        return false; // not an OpenSSH-format key
    }
    // layout after the magic: string ciphername; string kdfname; …
    let i = MAGIC.len();
    let n = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
    if i + 4 + n > bytes.len() {
        return false;
    }
    &bytes[i + 4..i + 4 + n] != b"none"
}

/// Import a private key file (+ its .pub when present). Returns Ok(true)
/// when the key was stored, Ok(false) when it already exists (skip).
///
/// A typed `passphrase` for an ENCRYPTED key is persisted as a separate
/// `ssh-pass:<stem>` secret (encrypted with the user key) so EVERY future
/// agent offer — session start, the 5-minute timer, the `S` flow — can
/// unlock the key non-interactively. Before D145 the typed passphrase was
/// discarded (`let _ = passphrase;`), so passphrase keys never reached the
/// agent without a manual terminal prompt.
pub async fn import_key_file(
    pool: &SqlitePool,
    user_id: &str,
    priv_path: &Path,
    pub_path: Option<&Path>,
    passphrase: Option<&str>,
) -> anyhow::Result<bool> {
    let stem = priv_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("key")
        .to_string();
    let priv_pem = std::fs::read_to_string(priv_path)?;
    let secret_name = format!("ssh:{}", stem);
    let existing = repository::list_secrets(pool, user_id).await?;
    if existing.iter().any(|sec| sec.name == secret_name) {
        return Ok(false);
    }
    let value_enc = secrets::encrypt_for_user(pool, user_id, &priv_pem).await?;
    let meta = SecretMeta {
        secret_group: Some("ssh".to_string()),
        username: None,
        url: None,
        email: None,
        passphrase_protected: is_passphrase_protected(&priv_pem),
        ssh_agent: true,
    };
    repository::create_secret_meta(
        pool,
        user_id,
        &secret_name,
        &value_enc,
        "ssh_key",
        false,
        &meta,
    )
    .await?;
    // D145: persist the typed passphrase (locked keys only).
    if meta.passphrase_protected {
        if let Some(pass) = passphrase.filter(|p| !p.is_empty()) {
            let pass_enc = secrets::encrypt_for_user(pool, user_id, pass).await?;
            let pass_meta = SecretMeta {
                secret_group: Some("ssh".to_string()),
                username: None,
                url: None,
                email: None,
                passphrase_protected: false,
                ssh_agent: false,
            };
            let _ = repository::create_secret_meta(
                pool,
                user_id,
                &format!("ssh-pass:{}", stem),
                &pass_enc,
                "password",
                false,
                &pass_meta,
            )
            .await;
        }
    }
    // Public half stored read-only for sharing (only when a .pub exists)
    if let Some(pub_path) = pub_path {
        if let Ok(pub_pem) = std::fs::read_to_string(pub_path) {
            if let Ok(pub_enc) = secrets::encrypt_for_user(pool, user_id, &pub_pem).await {
                let pub_meta = SecretMeta {
                    secret_group: Some("ssh".to_string()),
                    username: None,
                    url: None,
                    email: None,
                    passphrase_protected: false,
                    ssh_agent: false,
                };
                let _ = repository::create_secret_meta(
                    pool,
                    user_id,
                    &format!("ssh-pub:{}", stem),
                    &pub_enc,
                    "password",
                    false,
                    &pub_meta,
                )
                .await;
            }
        }
    }
    tracing::info!(key = %secret_name, "ssh key imported");
    Ok(true)
}
