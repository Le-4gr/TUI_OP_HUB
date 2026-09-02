//! Portable passphrase-based crypto for shared bundles (US-SEC-02).
//!
//! Independent of the machine-bound user key: bundles encrypted here can be
//! decrypted on any machine with the passphrase alone.

use crate::auth::EncryptionKey;
use crate::error::{AppError, AppResult};
use base64::Engine as _;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};

/// Derive a portable key from a passphrase (Argon2 with a fixed salt — the
/// salt is public by design here, the passphrase is the secret).
pub fn key_from_passphrase(passphrase: &str) -> AppResult<EncryptionKey> {
    let mut salt = [b't'; 16];
    salt[0] = b'o';
    salt[1] = b'h';
    EncryptionKey::from_password(passphrase, &salt)
}

/// Encrypt bytes with a portable key (nonce prepended, base64 encoded).
pub fn encrypt(plaintext: &[u8], key: &EncryptionKey) -> AppResult<String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| AppError::Other(format!("cipher init: {e}")))?;
    let nonce_bytes: [u8; 24] = rand::random();
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AppError::Other(format!("encrypt: {e}")))?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(&out))
}

/// Decrypt a base64 payload produced by [`encrypt`].
pub fn decrypt(encoded: &str, key: &EncryptionKey) -> AppResult<Vec<u8>> {
    let data = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| AppError::Validation(format!("bundle secret corrupt: {e}")))?;
    if data.len() < 24 {
        return Err(AppError::Validation("bundle secret too short".into()));
    }
    let (nonce_bytes, ct) = data.split_at(24);
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|e| AppError::Other(format!("cipher init: {e}")))?;
    cipher
        .decrypt(XNonce::from_slice(nonce_bytes), ct)
        .map_err(|_| AppError::Validation("wrong export passphrase".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passphrase_round_trip() {
        let key = key_from_passphrase("share-me").unwrap();
        let ct = encrypt(b"secret value", &key).unwrap();
        assert!(!ct.contains("share-me"));
        let pt = decrypt(&ct, &key).unwrap();
        assert_eq!(pt, b"secret value");
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let key = key_from_passphrase("right").unwrap();
        let wrong = key_from_passphrase("wrong").unwrap();
        let ct = encrypt(b"data", &key).unwrap();
        assert!(decrypt(&ct, &wrong).is_err());
    }

    #[test]
    fn ciphertext_is_not_deterministic() {
        let key = key_from_passphrase("p").unwrap();
        let a = encrypt(b"same", &key).unwrap();
        let b = encrypt(b"same", &key).unwrap();
        assert_ne!(a, b, "random nonce per encryption");
    }
}
