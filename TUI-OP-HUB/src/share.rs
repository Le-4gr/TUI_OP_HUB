//! Knowledge-base sharing (US-CMD-01, US-SEC-02).
//!
//! Export the knowledge base to portable JSON — with secrets **excluded**,
//! **encrypted** (portable: re-encrypted with an export passphrase instead of
//! the machine-bound key), or **plaintext** (explicit opt-in). Import merges
//! by (name, type); existing entries always win so local edits are safe.

use crate::error::{AppError, AppResult};
use crate::models::Entity;
use sqlx::SqlitePool;

/// How secrets are handled during export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretMode {
    Exclude,
    Encrypted,
    Plaintext,
}

/// A portable secret payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedSecret {
    pub name: String,
    pub kind: String,
    pub requires_reauth: bool,
    pub value: String,
}

/// The portable knowledge-base bundle.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeBundle {
    pub version: u32,
    pub exported_at: String,
    pub entities: Vec<SharedEntity>,
    #[serde(default)]
    pub secrets: Vec<SharedSecret>,
    pub secret_mode: String,
}

/// A portable entity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedEntity {
    pub name: String,
    pub type_id: String,
    pub description: Option<String>,
    pub content: Option<String>,
    /// Name of the parent command family (for option children).
    #[serde(default)]
    pub parent: Option<String>,
}

fn shared_from_entity(e: &Entity, parent: Option<String>) -> SharedEntity {
    SharedEntity {
        name: e.name.clone(),
        type_id: e.type_id.clone(),
        description: e.description.clone(),
        content: e.content.clone(),
        parent,
    }
}

/// Serialize the bundle to pretty JSON (what the user saves to a file).
pub fn bundle_to_json(bundle: &KnowledgeBundle) -> AppResult<String> {
    serde_json::to_string_pretty(bundle).map_err(|e| AppError::Other(format!("{e}")))
}

/// Parse a bundle from JSON text (what the user loads from a file).
/// Parse import JSON **leniently**. Accepted shapes:
///
/// 1. A full [`KnowledgeBundle`] (as produced by export)
/// 2. A bare JSON array of entities — the shape an AI generates from the
///    prompt in `docs/IMPORT_EXPORT.md` (missing bundle fields are defaulted)
/// 3. An object with only an `entities` array (`{"entities": [...]}`)
///
/// Defaults applied for missing fields: `version = 1`, `exported_at = now`,
/// `secret_mode = "excluded"`, empty `secrets`.
pub fn bundle_from_json(text: &str) -> AppResult<KnowledgeBundle> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| AppError::Validation(format!("invalid JSON: {e}")))?;

    // 1. Full bundle
    if let Ok(bundle) = serde_json::from_value::<KnowledgeBundle>(value.clone()) {
        return Ok(bundle);
    }

    // 2. Bare array of entities
    if value.is_array() {
        let entities: Vec<SharedEntity> = serde_json::from_value(value)
            .map_err(|e| AppError::Validation(format!("invalid entity array: {e}")))?;
        return Ok(defaulted_bundle(entities));
    }

    // 3. Object with an `entities` array (possibly missing other fields)
    if let Some(obj) = value.as_object() {
        if let Some(entities) = obj.get("entities") {
            let entities: Vec<SharedEntity> = serde_json::from_value(entities.clone())
                .map_err(|e| AppError::Validation(format!("invalid entities: {e}")))?;
            return Ok(defaulted_bundle(entities));
        }
    }

    Err(AppError::Validation(
        "expected a bundle, an array of entities, or an object with an `entities` array".into(),
    ))
}

/// A bundle filled with the safe defaults for lenient imports.
fn defaulted_bundle(entities: Vec<SharedEntity>) -> KnowledgeBundle {
    KnowledgeBundle {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        entities,
        secrets: Vec::new(),
        secret_mode: "excluded".to_string(),
    }
}

/// Export the knowledge base to a portable bundle.
pub async fn export_knowledge(
    pool: &SqlitePool,
    user_id: Option<&str>,
    secret_mode: SecretMode,
    export_passphrase: Option<&str>,
) -> AppResult<KnowledgeBundle> {
    let entities = crate::repository::list_entities(pool, None, None).await?;
    let mut shared: Vec<SharedEntity> = Vec::new();

    for e in entities.iter().filter(|e| e.parent_id.is_none()) {
        shared.push(shared_from_entity(e, None));
        for child in crate::repository::list_child_entities(pool, &e.id).await? {
            shared.push(shared_from_entity(&child, Some(e.name.clone())));
        }
    }

    let mut secrets_out: Vec<SharedSecret> = Vec::new();
    if let Some(uid) = user_id {
        let stored = crate::repository::list_secrets(pool, uid).await?;
        for s in stored {
            if secret_mode == SecretMode::Exclude {
                break;
            }
            let plaintext = match crate::secrets::decrypt_for_user_id(pool, uid, &s.value_enc).await
            {
                Ok(p) => p,
                Err(e) => {
                    return Err(AppError::Other(format!("decrypt failed: {e}")));
                }
            };
            // Encrypted mode re-encrypts with the export passphrase so the
            // bundle is portable (the machine-bound user key is NOT in it).
            let value = match secret_mode {
                SecretMode::Encrypted => {
                    let key = crate::share_crypto::key_from_passphrase(
                        export_passphrase.unwrap_or_default(),
                    )?;
                    crate::share_crypto::encrypt(plaintext.as_bytes(), &key)?
                }
                SecretMode::Plaintext => plaintext,
                SecretMode::Exclude => unreachable!(),
            };
            secrets_out.push(SharedSecret {
                name: s.name,
                kind: s.secret_kind,
                requires_reauth: s.requires_reauth,
                value,
            });
        }
    }

    let mode = match secret_mode {
        SecretMode::Exclude => "excluded",
        SecretMode::Encrypted => "encrypted",
        SecretMode::Plaintext => "plaintext",
    };
    Ok(KnowledgeBundle {
        version: 1,
        exported_at: chrono::Utc::now().to_rfc3339(),
        entities: shared,
        secrets: secrets_out,
        secret_mode: mode.to_string(),
    })
}

/// Import a bundle: merge entities by (name, type) — existing entries win so
/// local edits are never overwritten. Returns (imported, skipped) counts.
pub async fn import_knowledge(
    pool: &SqlitePool,
    bundle: &KnowledgeBundle,
) -> AppResult<(usize, usize)> {
    let mut imported = 0usize;
    let mut skipped = 0usize;

    // Pass 1: top-level entities
    for item in bundle.entities.iter().filter(|e| e.parent.is_none()) {
        if crate::repository::get_entity_by_name_and_type(pool, &item.name, &item.type_id)
            .await
            .is_ok()
        {
            skipped += 1;
            continue;
        }
        crate::repository::create_entity(
            pool,
            &crate::models::CreateEntity {
                name: item.name.clone(),
                description: item.description.clone(),
                content: item.content.clone(),
                type_id: item.type_id.clone(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await?;
        imported += 1;
    }

    // Pass 2: options/children, resolving the parent by name
    for item in bundle.entities.iter().filter(|e| e.parent.is_some()) {
        let parent_name = item.parent.as_deref().unwrap_or_default();
        let Ok(parent) =
            crate::repository::get_entity_by_name_and_type(pool, parent_name, "cmd").await
        else {
            skipped += 1;
            continue;
        };
        if crate::repository::get_entity_by_name_and_type(pool, &item.name, &item.type_id)
            .await
            .is_ok()
        {
            skipped += 1;
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO entities (id, name, description, content, type_id, parent_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&item.name)
        .bind(&item.description)
        .bind(&item.content)
        .bind(&item.type_id)
        .bind(&parent.id)
        .execute(pool)
        .await?;
        imported += 1;
    }

    Ok((imported, skipped))
}

/// Import secrets from a bundle. `Encrypted` secrets are decrypted with
/// the export passphrase and re-encrypted for the target user.
/// Returns (imported, skipped) counts.
pub async fn import_secrets(
    pool: &SqlitePool,
    user_id: &str,
    bundle: &KnowledgeBundle,
    export_passphrase: Option<&str>,
) -> AppResult<(usize, usize)> {
    let mut imported = 0usize;
    let mut skipped = 0usize;

    for secret in &bundle.secrets {
        let plaintext = match bundle.secret_mode.as_str() {
            "plaintext" => secret.value.clone(),
            "encrypted" => {
                let Some(pass) = export_passphrase else {
                    skipped += 1;
                    continue;
                };
                let key = crate::share_crypto::key_from_passphrase(pass)?;
                match crate::share_crypto::decrypt(&secret.value, &key) {
                    Ok(data) => String::from_utf8_lossy(&data).to_string(),
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                }
            }
            _ => {
                skipped += 1;
                continue;
            }
        };
        let enc = match crate::secrets::encrypt_for_user(pool, user_id, &plaintext).await {
            Ok(enc) => enc,
            Err(e) => return Err(AppError::Other(format!("encrypt: {e}"))),
        };
        crate::repository::create_secret_full(
            pool,
            user_id,
            &secret.name,
            &enc,
            &secret.kind,
            secret.requires_reauth,
        )
        .await?;
        imported += 1;
    }

    Ok((imported, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario: a full exported bundle parses unchanged
    #[test]
    fn given_full_bundle_when_parsed_then_fields_preserved() {
        let text = r#"{
            "version": 1,
            "exported_at": "2026-01-01T00:00:00Z",
            "secret_mode": "excluded",
            "entities": [
                {"name": "ls", "type_id": "cmd", "description": "list",
                 "content": "ls -la", "parent": null}
            ],
            "secrets": []
        }"#;
        let bundle = bundle_from_json(text).unwrap();
        assert_eq!(bundle.version, 1);
        assert_eq!(bundle.entities.len(), 1);
        assert_eq!(bundle.entities[0].name, "ls");
        assert_eq!(bundle.secret_mode, "excluded");
    }

    /// Scenario: a bare AI-generated entity array imports with defaults
    #[test]
    fn given_bare_entity_array_when_parsed_then_defaults_applied() {
        let script = "#!/bin/sh\nrsync -a ~/ /mnt/nas";
        let text = format!(
            "[\n  {{\"name\": \"docker ps\", \"type_id\": \"cmd\", \"description\": \"list containers\", \"content\": \"docker ps\"}},\n  {{\"name\": \"backup\", \"type_id\": \"script\", \"content\": {:?}}}\n]",
            script
        );
        let bundle = bundle_from_json(&text).unwrap();
        assert_eq!(bundle.version, 1);
        assert_eq!(bundle.secret_mode, "excluded");
        assert!(bundle.secrets.is_empty());
        assert_eq!(bundle.entities.len(), 2);
        assert_eq!(bundle.entities[0].type_id, "cmd");
        assert_eq!(bundle.entities[1].description, None);
        assert_eq!(bundle.entities[1].content.as_deref(), Some(script));
    }

    /// Scenario: an entities-only object imports with defaults
    #[test]
    fn given_entities_only_object_when_parsed_then_defaults_applied() {
        let text = r#"{"entities": [
            {"name": "htop", "type_id": "app", "content": "htop"}
        ]}"#;
        let bundle = bundle_from_json(text).unwrap();
        assert_eq!(bundle.entities.len(), 1);
        assert_eq!(bundle.entities[0].type_id, "app");
        assert_eq!(bundle.secret_mode, "excluded");
    }

    /// Scenario: malformed input is rejected with a useful message
    #[test]
    fn given_garbage_when_parsed_then_validation_error() {
        assert!(bundle_from_json("not json").is_err());
        assert!(bundle_from_json("{}").is_err()); // object without entities
        assert!(bundle_from_json(r#"{"entities": "nope"}"#).is_err());
        // entity missing required fields
        assert!(bundle_from_json(r#"[{"name": "x"}]"#).is_err());
    }
}
