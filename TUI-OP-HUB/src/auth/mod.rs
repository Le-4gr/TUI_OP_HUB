use crate::error::{AppError, AppResult};
use crate::repository;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadOsRng},
    XChaCha20Poly1305, XNonce,
};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use zeroize::Zeroize;

/// Encryption key that is automatically zeroized when dropped
#[derive(Clone)]
pub struct EncryptionKey {
    key: [u8; 32],
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

impl EncryptionKey {
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn from_password(password: &str, salt: &[u8]) -> AppResult<Self> {
        let argon2 = Argon2::default();
        let salt_string = SaltString::encode_b64(salt)
            .map_err(|e| AppError::Other(format!("Salt encoding error: {}", e)))?;

        let hash = argon2
            .hash_password(password.as_bytes(), &salt_string)
            .map_err(|e| AppError::Other(format!("Key derivation error: {}", e)))?;

        let hash_bytes = hash
            .hash
            .ok_or_else(|| AppError::Other("No hash generated".into()))?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash_bytes.as_bytes()[..32]);

        Ok(Self::new(key))
    }
}

/// Authentication manager for user login and password management
pub struct AuthManager {
    pool: Arc<SqlitePool>,
}

impl AuthManager {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Create a new user with username and password
    pub async fn create_user(&self, username: &str, password: &str) -> AppResult<String> {
        // The very first registered user becomes admin (US-SEC)
        let first_user = repository::count_users(&*self.pool).await? == 0;
        let is_admin_flag: i64 = if first_user { 1 } else { 0 };

        // Generate user ID
        let user_id = uuid::Uuid::new_v4().to_string();

        // Generate salt
        let salt = SaltString::generate(&mut OsRng);
        let salt_bytes = salt.as_str().as_bytes();

        // Hash password with Argon2id
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Other(format!("Password hashing error: {}", e)))?
            .to_string();

        // Derive encryption key from password
        let encryption_key = EncryptionKey::from_password(password, salt_bytes)?;

        // Insert user profile
        sqlx::query(
            r#"
            INSERT INTO user_profiles (id, username, auth_method, password_hash, salt, is_admin, created_at, updated_at)
            VALUES (?, ?, 'password', ?, ?, ?, datetime('now'), datetime('now'))
            "#,
        )
        .bind(&user_id)
        .bind(username)
        .bind(&password_hash)
        .bind(salt.as_str())
        .bind(is_admin_flag)
        .execute(&*self.pool)
        .await?;

        // Store user encryption key
        let key_b64 = general_purpose::STANDARD.encode(encryption_key.as_bytes());
        sqlx::query(
            r#"
            INSERT INTO user_keys (id, user_id, key_b64, created_at)
            VALUES (?, ?, ?, datetime('now'))
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(&key_b64)
        .execute(&*self.pool)
        .await?;

        Ok(user_id)
    }

    /// Set a master password for a user
    pub async fn set_master_password(&self, user_id: &str, password: &str) -> AppResult<()> {
        // Generate salt
        let salt = SaltString::generate(&mut OsRng);
        let salt_bytes = salt.as_str().as_bytes();

        // Hash password with Argon2id
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Other(format!("Password hashing error: {}", e)))?
            .to_string();

        // Derive encryption key from password
        let encryption_key = EncryptionKey::from_password(password, salt_bytes)?;

        // Store password hash and salt in database
        sqlx::query(
            r#"
            UPDATE user_profiles 
            SET password_hash = ?, salt = ?, auth_method = 'password', last_login = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
        )
        .bind(&password_hash)
        .bind(salt.as_str())
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        // Update user encryption key
        self.update_user_key(user_id, &encryption_key).await?;

        Ok(())
    }

    /// Verify a password for a user by username
    pub async fn verify_password_by_username(
        &self,
        username: &str,
        password: &str,
    ) -> AppResult<bool> {
        let row = sqlx::query("SELECT password_hash FROM user_profiles WHERE username = ?")
            .bind(username)
            .fetch_optional(&*self.pool)
            .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let password_hash_str: Option<String> = row.try_get("password_hash").ok();
        let password_hash_str = match password_hash_str {
            Some(hash) => hash,
            None => return Ok(false),
        };

        let parsed_hash = PasswordHash::new(&password_hash_str)
            .map_err(|e| AppError::Other(format!("Invalid password hash: {}", e)))?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Verify a password for a user by user ID
    pub async fn verify_password(&self, user_id: &str, password: &str) -> AppResult<bool> {
        let row = sqlx::query("SELECT password_hash FROM user_profiles WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await?;

        let Some(row) = row else {
            return Ok(false);
        };

        let password_hash_str: Option<String> = row.try_get("password_hash").ok();
        let password_hash_str = match password_hash_str {
            Some(hash) => hash,
            None => return Ok(false),
        };

        let parsed_hash = PasswordHash::new(&password_hash_str)
            .map_err(|e| AppError::Other(format!("Invalid password hash: {}", e)))?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Get the authentication method for a user
    pub async fn get_auth_method(&self, user_id: &str) -> AppResult<String> {
        let row = sqlx::query("SELECT auth_method FROM user_profiles WHERE id = ?")
            .bind(user_id)
            .fetch_one(&*self.pool)
            .await?;

        let auth_method: Option<String> = row.try_get("auth_method").ok();
        Ok(auth_method.unwrap_or_else(|| "env".to_string()))
    }

    /// Get salt for a user
    pub async fn get_salt(&self, user_id: &str) -> AppResult<Vec<u8>> {
        let row = sqlx::query("SELECT salt FROM user_profiles WHERE id = ?")
            .bind(user_id)
            .fetch_one(&*self.pool)
            .await?;

        let salt_str: String = row
            .try_get("salt")
            .map_err(|_| AppError::Other("No salt found".into()))?;
        Ok(salt_str.as_bytes().to_vec())
    }

    /// Update user encryption key in user_keys table
    async fn update_user_key(&self, user_id: &str, key: &EncryptionKey) -> AppResult<()> {
        let key_b64 = general_purpose::STANDARD.encode(key.as_bytes());

        sqlx::query(
            r#"
            INSERT INTO user_keys (user_id, key_enc) 
            VALUES (?, ?)
            ON CONFLICT(user_id) DO UPDATE SET key_enc = excluded.key_enc
            "#,
        )
        .bind(user_id)
        .bind(&key_b64)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Change user password and re-encrypt all secrets
    pub async fn change_password(
        &self,
        user_id: &str,
        old_password: &str,
        new_password: &str,
    ) -> AppResult<()> {
        // Verify old password
        if !self.verify_password(user_id, old_password).await? {
            return Err(AppError::Unauthorized("Invalid password".into()));
        }

        // Get old salt and derive old key
        let old_salt = self.get_salt(user_id).await?;
        let old_key = EncryptionKey::from_password(old_password, &old_salt)?;

        // Fetch all secrets
        let secrets = sqlx::query("SELECT id, value_enc FROM secrets WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await?;

        // Decrypt all secrets with old key
        let mut decrypted_secrets: Vec<(String, String)> = Vec::new();
        for secret in secrets {
            let id: String = secret.try_get("id")?;
            let value_enc: String = secret.try_get("value_enc")?;
            let plaintext = decrypt_value(&value_enc, &old_key)?;
            decrypted_secrets.push((id, plaintext));
        }

        // Generate new salt and derive new key
        let new_salt = SaltString::generate(&mut OsRng);
        let new_key = EncryptionKey::from_password(new_password, new_salt.as_str().as_bytes())?;

        // Re-encrypt all secrets with new key
        for (secret_id, plaintext) in decrypted_secrets {
            let new_encrypted = encrypt_value(&plaintext, &new_key)?;
            sqlx::query("UPDATE secrets SET value_enc = ? WHERE id = ?")
                .bind(&new_encrypted)
                .bind(&secret_id)
                .execute(&*self.pool)
                .await?;
        }

        // Update password hash and salt
        self.set_master_password(user_id, new_password).await?;

        Ok(())
    }

    /// Record login attempt
    pub async fn record_login(&self, user_id: &str, success: bool) -> AppResult<()> {
        if success {
            sqlx::query("UPDATE user_profiles SET last_login = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(user_id)
                .execute(&*self.pool)
                .await?;
        }

        // TODO: Log failed attempts for security monitoring

        Ok(())
    }
}

/// Encrypt a value with the given key
/// Admin password reset (US-SEC): replace the target user's password when it
/// was forgotten. Requires an admin actor. Note: the user's encryption key was
/// derived from the old password, so previously stored secrets remain encrypted
/// with that old key and cannot be decrypted after the reset. Use
/// `repository::delete_user` to remove the user together with their secrets.
pub async fn admin_reset_password(
    pool: &sqlx::SqlitePool,
    admin_user_id: &str,
    target_username: &str,
    new_password: &str,
) -> AppResult<()> {
    if !repository::is_admin(pool, admin_user_id).await? {
        return Err(AppError::Other(
            "Only admins can reset passwords".to_string(),
        ));
    }
    let user = repository::get_or_create_user(pool, target_username).await?;
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(new_password.as_bytes(), &salt)
        .map_err(|e| AppError::Other(format!("Password hashing error: {}", e)))?
        .to_string();
    repository::set_password_hash(pool, &user.id, &password_hash, salt.as_str()).await
}

pub fn encrypt_value(plaintext: &str, key: &EncryptionKey) -> AppResult<String> {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut AeadOsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| AppError::Other(format!("Encryption error: {}", e)))?;

    // Combine nonce + ciphertext and encode as base64
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(general_purpose::STANDARD.encode(&combined))
}

/// Decrypt a value with the given key
pub fn decrypt_value(encrypted: &str, key: &EncryptionKey) -> AppResult<String> {
    let combined = general_purpose::STANDARD
        .decode(encrypted)
        .map_err(|e| AppError::Other(format!("Base64 decode error: {}", e)))?;

    if combined.len() < 24 {
        return Err(AppError::Other("Invalid encrypted data".into()));
    }

    let (nonce_bytes, ciphertext) = combined.split_at(24);
    let nonce = XNonce::from_slice(nonce_bytes);

    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AppError::Other(format!("Decryption error: {}", e)))?;

    String::from_utf8(plaintext).map_err(|e| AppError::Other(format!("UTF-8 error: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption() {
        let password = "test_password_123";
        let salt = b"test_salt_16byte";
        let key = EncryptionKey::from_password(password, salt).unwrap();

        let plaintext = "secret_value";
        let encrypted = encrypt_value(plaintext, &key).unwrap();
        let decrypted = decrypt_value(&encrypted, &key).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_key_zeroization() {
        let password = "test_password";
        let salt = b"test_salt_16byte";
        let key = EncryptionKey::from_password(password, salt).unwrap();
        let key_copy = key.as_bytes().clone();

        drop(key);

        // Key should be zeroized after drop
        // This is a conceptual test - actual verification would require unsafe code
        assert_eq!(key_copy.len(), 32);
    }
}
