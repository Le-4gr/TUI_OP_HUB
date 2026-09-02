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
pub fn bundle_from_json(text: &str) -> AppResult<KnowledgeBundle> {
    serde_json::from_str(text).map_err(|e| AppError::Validation(format!("invalid bundle: {e}")))
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
