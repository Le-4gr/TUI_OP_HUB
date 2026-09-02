//! Idempotent prepopulation of the knowledge base (US-CMD-01, US-SRCH, US-PROC).
//!
//! Seeds **command families** (`cmd` entities) with their **options** as child
//! entities (`opt` type, linked via `entities.parent_id`) — each option carries
//! its own description. Also seeds known desktop/TUI tools as `app` entities
//! (file browsers like yazi/ranger/lf, editors, fetch tools, process viewers)
//! so the hub is useful out of the box. Covers both systemd (`systemctl`,
//! `journalctl`) and OpenRC (`rc-service`, `rc-update`) init systems.
//!
//! Seeding is idempotent: families/options/tools are checked by name before
//! insert, and a `seed_meta` marker short-circuits repeated runs.

use crate::error::AppResult;
use crate::seed_data::seed_commands;
use crate::seed_data::SEED_TOOLS;
use sqlx::SqlitePool;

/// Seed data version — bump to re-run seeds with new content.
pub const SEED_VERSION: &str = "commands_v1";

/// Detect the running init system (systemd / OpenRC / unknown). Shown in the
/// fetch panel and used to pick sensible service commands.
pub fn detect_init_system() -> &'static str {
    if std::path::Path::new("/run/systemd/system").exists() || crate::keygen::which("systemctl") {
        "systemd"
    } else if crate::keygen::which("rc-service") || std::path::Path::new("/sbin/openrc").exists() {
        "openrc"
    } else {
        "unknown"
    }
}

/// Seed command families, options and known tools into the database.
/// Idempotent — safe to call on every startup.
pub async fn seed_builtin_commands(pool: &SqlitePool) -> AppResult<()> {
    // Fast path: already seeded with this version
    let seeded: Option<(String,)> =
        sqlx::query_as("SELECT value FROM seed_meta WHERE key = 'commands'")
            .fetch_optional(pool)
            .await?;
    if seeded.map(|(v,)| v == SEED_VERSION).unwrap_or(false) {
        return Ok(());
    }

    for family in seed_commands() {
        // Family entity (skip when it already exists — user may have edited it)
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM entities WHERE name = ? AND type_id = 'cmd' AND parent_id IS NULL",
        )
        .bind(family.name)
        .fetch_optional(pool)
        .await?;
        let family_id = match existing {
            Some((id,)) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO entities (id, name, description, content, type_id) VALUES (?, ?, ?, ?, 'cmd')",
                )
                .bind(&id)
                .bind(family.name)
                .bind(family.description)
                .bind(family.name) // content defaults to the command itself
                .execute(pool)
                .await?;
                id
            }
        };

        // Options as child entities (skip existing ones)
        for option in family.options {
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM entities WHERE name = ? AND type_id = 'opt' AND parent_id = ?",
            )
            .bind(option.flag)
            .bind(&family_id)
            .fetch_optional(pool)
            .await?;
            if exists.is_none() {
                sqlx::query(
                    "INSERT INTO entities (id, name, description, content, type_id, parent_id) VALUES (?, ?, ?, ?, 'opt', ?)",
                )
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(option.flag)
                .bind(option.description)
                .bind(format!("{} {}", family.name, option.flag))
                .bind(&family_id)
                .execute(pool)
                .await?;
            }
        }
    }

    // Known tools as `app` entities, tagged for filtering
    for (name, description, tag) in SEED_TOOLS {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT id FROM entities WHERE name = ? AND type_id = 'app'")
                .bind(name)
                .fetch_optional(pool)
                .await?;
        if exists.is_none() {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO entities (id, name, description, content, type_id) VALUES (?, ?, ?, ?, 'app')",
            )
            .bind(&id)
            .bind(name)
            .bind(description)
            .bind(name) // launching an app runs the binary itself
            .execute(pool)
            .await?;
            sqlx::query("INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(tag)
                .execute(pool)
                .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO entity_tags (entity_id, tag_id) SELECT ?, id FROM tags WHERE name = ?",
            )
            .bind(&id)
            .bind(tag)
            .execute(pool)
            .await?;
        }
    }

    // Mark the seed version
    sqlx::query(
        "INSERT INTO seed_meta (key, value) VALUES ('commands', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(SEED_VERSION)
    .execute(pool)
    .await?;

    tracing::info!("knowledge base seeded (version {})", SEED_VERSION);
    Ok(())
}
