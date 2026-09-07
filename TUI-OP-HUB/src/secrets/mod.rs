use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::XChaCha20Poly1305;
use chacha20poly1305::XNonce;
use sqlx::SqlitePool;

pub mod ssh_agent;

// Retrieve user's encryption key from DB or env fallback
pub async fn get_user_key(pool: &SqlitePool, user_id: &str) -> anyhow::Result<String> {
    // First try to get from DB
    if let Ok(Some((key_b64,))) =
        sqlx::query_as::<_, (String,)>("SELECT key_b64 FROM user_keys WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
    {
        return Ok(key_b64);
    }
    // Fallback to env var
    std::env::var("TUI_OP_HUB_SECRETS_KEY")
        .map_err(|_| anyhow::anyhow!("TUI_OP_HUB_SECRETS_KEY not set and no user key in DB"))
}

pub async fn encrypt_for_user(
    pool: &SqlitePool,
    user_id: &str,
    plaintext: &str,
) -> anyhow::Result<String> {
    let key_b64 = get_user_key(pool, user_id).await?;
    let key_bytes = general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|e| anyhow::anyhow!(e))?;
    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("invalid key length"));
    }
    let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!(format!("key init error: {:?}", e)))?;
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|e| anyhow::anyhow!(e))?;
    let nonce_ref = XNonce::from_slice(&nonce);
    let ciphertext = cipher
        .encrypt(nonce_ref, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!(format!("encrypt error: {:?}", e)))?;
    let mut out = Vec::new();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(general_purpose::STANDARD.encode(&out))
}

pub async fn decrypt_for_user(
    pool: &SqlitePool,
    user_id: &str,
    value_enc_b64: &str,
) -> anyhow::Result<String> {
    let key_b64 = get_user_key(pool, user_id).await?;
    let key_bytes = general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|e| anyhow::anyhow!(e))?;
    if key_bytes.len() != 32 {
        return Err(anyhow::anyhow!("invalid key length"));
    }
    let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes)
        .map_err(|e| anyhow::anyhow!(format!("key init error: {:?}", e)))?;
    let data = general_purpose::STANDARD
        .decode(value_enc_b64)
        .map_err(|e| anyhow::anyhow!(e))?;
    if data.len() < 24 {
        return Err(anyhow::anyhow!("invalid ciphertext"));
    }
    let (nonce_bytes, ct) = data.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!(format!("decrypt error: {:?}", e)))?;
    let s = String::from_utf8(pt).map_err(|e| anyhow::anyhow!(e))?;
    Ok(s)
}

// Legacy functions (use default user)
pub async fn encrypt(pool: &SqlitePool, plaintext: &str) -> anyhow::Result<String> {
    encrypt_for_user(pool, "default", plaintext).await
}

pub async fn decrypt(pool: &SqlitePool, value_enc_b64: &str) -> anyhow::Result<String> {
    decrypt_for_user(pool, "default", value_enc_b64).await
}

/// Wrap a plaintext with a passphrase-derived key (share_crypto: Argon2 +
/// XChaCha20-Poly1305). The result is an opaque string that the caller then
/// stores through the normal user-key encryption, giving two independent
/// layers: the machine user key AND the secret's own passphrase (US-SEC).
pub fn wrap_with_passphrase(plaintext: &str, passphrase: &str) -> anyhow::Result<String> {
    if passphrase.is_empty() {
        anyhow::bail!("passphrase must not be empty");
    }
    let key = crate::share_crypto::key_from_passphrase(passphrase)?;
    crate::share_crypto::encrypt(plaintext.as_bytes(), &key).map_err(|e| anyhow::anyhow!("{}", e))
}

/// Unwrap a passphrase-protected inner layer. Exact inverse of
/// [`wrap_with_passphrase`]; a wrong passphrase fails authentication.
pub fn unwrap_with_passphrase(wrapped: &str, passphrase: &str) -> anyhow::Result<String> {
    let key = crate::share_crypto::key_from_passphrase(passphrase)?;
    let bytes =
        crate::share_crypto::decrypt(wrapped, &key).map_err(|e| anyhow::anyhow!("{}", e))?;
    String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("invalid utf-8: {e}"))
}

#[cfg(test)]
mod passphrase_tests {
    use super::*;

    #[test]
    fn given_passphrase_when_wrapped_then_unwrap_round_trips() {
        let wrapped = wrap_with_passphrase("hunter2 secret", "correct horse").unwrap();
        assert_ne!(wrapped, "hunter2 secret");
        let out = unwrap_with_passphrase(&wrapped, "correct horse").unwrap();
        assert_eq!(out, "hunter2 secret");
    }

    #[test]
    fn given_wrong_passphrase_when_unwrapped_then_fails() {
        let wrapped = wrap_with_passphrase("top secret", "right").unwrap();
        assert!(unwrap_with_passphrase(&wrapped, "wrong").is_err());
    }

    #[test]
    fn given_empty_passphrase_when_wrapped_then_rejected() {
        assert!(wrap_with_passphrase("x", "").is_err());
    }
}
