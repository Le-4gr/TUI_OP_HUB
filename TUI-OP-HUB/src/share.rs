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

/// How to handle entities whose (name, type) already exists locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DuplicateMode {
    /// Keep the local version, ignore the incoming one (safest, default).
    #[default]
    Skip,
    /// Replace the local version with the incoming one.
    Overwrite,
    /// Import the incoming entity under a `-imported` suffixed name.
    Rename,
}

impl DuplicateMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DuplicateMode::Skip => "skip",
            DuplicateMode::Overwrite => "overwrite",
            DuplicateMode::Rename => "rename",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            DuplicateMode::Skip => DuplicateMode::Overwrite,
            DuplicateMode::Overwrite => DuplicateMode::Rename,
            DuplicateMode::Rename => DuplicateMode::Skip,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "skip" => Some(DuplicateMode::Skip),
            "overwrite" => Some(DuplicateMode::Overwrite),
            "rename" => Some(DuplicateMode::Rename),
            _ => None,
        }
    }
}

/// Result of an import run.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub overwritten: usize,
    pub renamed: usize,
}

/// Import a bundle (skip-duplicates default) — legacy wrapper.
pub async fn import_knowledge(
    pool: &SqlitePool,
    bundle: &KnowledgeBundle,
) -> AppResult<(usize, usize)> {
    let report = import_knowledge_with_mode(pool, bundle, DuplicateMode::Skip).await?;
    Ok((report.imported, report.skipped))
}

/// Import a bundle with an explicit duplicate strategy (US-CMD-01).
/// Merge key is (name, type); children attach to parents by name.
pub async fn import_knowledge_with_mode(
    pool: &SqlitePool,
    bundle: &KnowledgeBundle,
    mode: DuplicateMode,
) -> AppResult<ImportReport> {
    let mut report = ImportReport::default();

    // Pass 1: top-level entities
    for item in bundle.entities.iter().filter(|e| e.parent.is_none()) {
        let existing =
            crate::repository::get_entity_by_name_and_type(pool, &item.name, &item.type_id).await;
        let incoming = crate::models::CreateEntity {
            name: item.name.clone(),
            description: item.description.clone(),
            content: item.content.clone(),
            type_id: item.type_id.clone(),
            project_id: None,
            tags: None,
            metadata_json: None,
        };
        if let Ok(existing) = existing {
            match mode {
                DuplicateMode::Skip => {
                    report.skipped += 1;
                    continue;
                }
                DuplicateMode::Overwrite => {
                    crate::repository::update_entity(pool, &existing.id, &incoming).await?;
                    report.overwritten += 1;
                    continue;
                }
                DuplicateMode::Rename => {
                    // fall through: create under a deduplicated name
                }
            }
        }
        let name = if matches!(mode, DuplicateMode::Rename) {
            let mut candidate = item.name.clone();
            let mut n = 1;
            loop {
                let suffix = if n == 1 {
                    "-imported".to_string()
                } else {
                    format!("-imported-{}", n)
                };
                candidate = format!("{}{}", item.name, suffix);
                if crate::repository::get_entity_by_name_and_type(pool, &candidate, &item.type_id)
                    .await
                    .is_err()
                {
                    break;
                }
                n += 1;
            }
            candidate
        } else {
            item.name.clone()
        };
        let mut incoming = incoming;
        incoming.name = name;
        crate::repository::create_entity(pool, &incoming).await?;
        if matches!(mode, DuplicateMode::Rename) {
            report.renamed += 1;
        } else {
            report.imported += 1;
        }
    }

    // Pass 2: options/children, resolving the parent by name (original or
    // the -imported rename produced above).
    for item in bundle.entities.iter().filter(|e| e.parent.is_some()) {
        let parent_name = item.parent.as_deref().unwrap_or_default();
        let parent = match crate::repository::get_entity_by_name_and_type(pool, parent_name, "cmd")
            .await
        {
            Ok(p) => p,
            Err(_) => {
                // Parent may have been renamed during this import
                let renamed = format!("{}-imported", parent_name);
                match crate::repository::get_entity_by_name_and_type(pool, &renamed, "cmd").await {
                    Ok(p) => p,
                    Err(_) => {
                        report.skipped += 1;
                        continue;
                    }
                }
            }
        };
        let existing =
            crate::repository::get_entity_by_name_and_type(pool, &item.name, &item.type_id).await;
        let incoming = crate::models::CreateEntity {
            name: item.name.clone(),
            description: item.description.clone(),
            content: item.content.clone(),
            type_id: item.type_id.clone(),
            project_id: None,
            tags: None,
            metadata_json: None,
        };
        if let Ok(existing) = existing {
            match mode {
                DuplicateMode::Skip => {
                    report.skipped += 1;
                    continue;
                }
                DuplicateMode::Overwrite => {
                    crate::repository::update_entity(pool, &existing.id, &incoming).await?;
                    report.overwritten += 1;
                    continue;
                }
                DuplicateMode::Rename => {
                    // fall through to create under a deduplicated name
                }
            }
        }
        let name = if matches!(mode, DuplicateMode::Rename) {
            let mut candidate = item.name.clone();
            let mut n = 1;
            loop {
                let suffix = if n == 1 {
                    "-imported".to_string()
                } else {
                    format!("-imported-{}", n)
                };
                candidate = format!("{}{}", item.name, suffix);
                if crate::repository::get_entity_by_name_and_type(pool, &candidate, &item.type_id)
                    .await
                    .is_err()
                {
                    break;
                }
                n += 1;
            }
            candidate
        } else {
            item.name.clone()
        };
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO entities (id, name, description, content, type_id, parent_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&item.description)
        .bind(&item.content)
        .bind(&item.type_id)
        .bind(&parent.id)
        .execute(pool)
        .await?;
        if matches!(mode, DuplicateMode::Rename) {
            report.renamed += 1;
        } else {
            report.imported += 1;
        }
    }

    Ok(report)
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
