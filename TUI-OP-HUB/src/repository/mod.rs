//! Repository layer (US-CMD-01..09, US-PROJ-01..06, US-SRCH-01..04).
use crate::error::{AppError, AppResult};
use crate::models::*;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn create_entity(pool: &SqlitePool, req: &CreateEntity) -> AppResult<Entity> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entities (id, name, description, content, type_id, project_id, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.content)
    .bind(&req.type_id)
    .bind(&req.project_id)
    .bind(&req.metadata_json)
    .execute(pool).await?;

    if let Some(tags) = &req.tags {
        for tag_name in tags {
            let tag_id = Uuid::new_v4().to_string();
            sqlx::query("INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)")
                .bind(&tag_id)
                .bind(tag_name)
                .execute(pool)
                .await?;
            sqlx::query("INSERT OR IGNORE INTO entity_tags (entity_id, tag_id) SELECT ?, id FROM tags WHERE name = ?")
                .bind(&id).bind(tag_name).execute(pool).await?;
        }
    }
    get_entity(pool, &id).await
}

pub async fn get_entity(pool: &SqlitePool, id: &str) -> AppResult<Entity> {
    sqlx::query_as::<_, Entity>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound {
            entity: "entity",
            id: id.to_string(),
        })
}

pub async fn list_entities(
    pool: &SqlitePool,
    type_filter: Option<&str>,
    project_id: Option<&str>,
) -> AppResult<Vec<Entity>> {
    let rows = sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE (? IS NULL OR type_id = ?) AND (? IS NULL OR project_id = ?) ORDER BY created_at DESC"
    )
    .bind(type_filter).bind(type_filter)
    .bind(project_id).bind(project_id)
    .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_entities_by_project(
    pool: &SqlitePool,
    project_id: &str,
) -> AppResult<Vec<Entity>> {
    let rows = sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE project_id = ? ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_entity(pool: &SqlitePool, id: &str, req: &CreateEntity) -> AppResult<Entity> {
    sqlx::query(
        "UPDATE entities SET name = ?, description = ?, content = ?, type_id = ?, project_id = ?, metadata_json = ?, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(&req.name).bind(&req.description).bind(&req.content)
    .bind(&req.type_id).bind(&req.project_id).bind(&req.metadata_json).bind(id)
    .execute(pool).await?;
    get_entity(pool, id).await
}

pub async fn delete_entity(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM entities WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "entity",
            id: id.to_string(),
        });
    }
    Ok(())
}

pub async fn create_project(pool: &SqlitePool, req: &CreateProject) -> AppResult<Project> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO projects (id, name, description) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(&req.name)
        .bind(&req.description)
        .execute(pool)
        .await?;
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn get_project(pool: &SqlitePool, id: &str) -> AppResult<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound {
            entity: "project",
            id: id.to_string(),
        })
}

pub async fn list_projects(pool: &SqlitePool) -> AppResult<Vec<Project>> {
    let rows = sqlx::query_as::<_, Project>("SELECT * FROM projects ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn delete_project(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "project",
            id: id.to_string(),
        });
    }
    Ok(())
}

pub async fn update_project(
    pool: &SqlitePool,
    id: &str,
    req: &CreateProject,
) -> AppResult<Project> {
    sqlx::query(
        "UPDATE projects SET name = ?, description = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(id)
    .execute(pool)
    .await?;
    get_project(pool, id).await
}

/// Set the project environment (US-ENV-01): `env_type` like `venv`/`pyenv`/
/// `conda` and the shell command that activates it, e.g. `source .venv/bin/activate`.
pub async fn set_project_env(
    pool: &SqlitePool,
    id: &str,
    env_type: Option<&str>,
    env_cmd: Option<&str>,
) -> AppResult<Project> {
    let result = sqlx::query(
        "UPDATE projects SET env_type = ?, env_cmd = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(env_type)
    .bind(env_cmd)
    .bind(id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "project",
            id: id.to_string(),
        });
    }
    get_project(pool, id).await
}

pub async fn list_tags(pool: &SqlitePool) -> AppResult<Vec<Tag>> {
    let rows = sqlx::query_as::<_, Tag>("SELECT * FROM tags ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn list_types(pool: &SqlitePool) -> AppResult<Vec<EntityType>> {
    let rows = sqlx::query_as::<_, EntityType>("SELECT * FROM types ORDER BY name")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn search_entities(pool: &SqlitePool, query: &str) -> AppResult<Vec<SearchResult>> {
    let rows = sqlx::query_as::<_, SearchResult>(
        "SELECT e.id, e.name, e.description, e.content, e.type_id FROM entities e JOIN entities_fts f ON e.rowid = f.rowid WHERE entities_fts MATCH ? ORDER BY rank"
    )
    .bind(query)
    .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn filter_by_tags(pool: &SqlitePool, tag_names: &[String]) -> AppResult<Vec<Entity>> {
    if tag_names.is_empty() {
        return list_entities(pool, None, None).await;
    }
    let placeholders = tag_names.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT DISTINCT e.* FROM entities e JOIN entity_tags et ON e.id = et.entity_id JOIN tags t ON et.tag_id = t.id WHERE t.name IN ({}) ORDER BY e.created_at DESC",
        placeholders
    );
    let mut q = sqlx::query_as::<_, Entity>(&sql);
    for name in tag_names {
        q = q.bind(name);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

pub async fn get_entity_tags(pool: &SqlitePool, entity_id: &str) -> AppResult<Vec<Tag>> {
    let rows = sqlx::query_as::<_, Tag>(
        "SELECT t.* FROM tags t JOIN entity_tags et ON t.id = et.tag_id WHERE et.entity_id = ? ORDER BY t.name"
    )
    .bind(entity_id)
    .fetch_all(pool).await?;
    Ok(rows)
}

// Persist a workflow run result (workflow_runs table)
pub async fn insert_workflow_run(
    pool: &SqlitePool,
    run: &crate::models::WorkflowRun,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO workflow_runs (run_id, workflow_id, success, output, error, duration_ms, steps_completed, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&run.run_id)
    .bind(&run.workflow_id)
    .bind(if run.success { 1 } else { 0 })
    .bind(&run.output)
    .bind(&run.error)
    .bind(run.duration_ms)
    .bind(run.steps_completed)
    .bind(&run.created_at)
    .execute(pool).await?;
    Ok(())
}

pub async fn list_workflow_runs_by_workflow_id(
    pool: &SqlitePool,
    workflow_id: &str,
) -> AppResult<Vec<crate::models::WorkflowRun>> {
    let rows = sqlx::query_as::<_, crate::models::WorkflowRun>(
        "SELECT * FROM workflow_runs WHERE workflow_id = ? ORDER BY created_at DESC",
    )
    .bind(workflow_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// Secrets CRUD (with user_id)
pub async fn create_secret(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
    value_enc: &str,
) -> AppResult<crate::models::Secret> {
    create_secret_full(pool, user_id, name, value_enc, "password", false).await
}

/// Create a secret with classification and access control (US-SEC-01/05).
pub async fn create_secret_full(
    pool: &SqlitePool,
    user_id: &str,
    name: &str,
    value_enc: &str,
    secret_kind: &str,
    requires_reauth: bool,
) -> AppResult<crate::models::Secret> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO secrets (id, user_id, name, value_enc, secret_kind, requires_reauth) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(value_enc)
    .bind(secret_kind)
    .bind(if requires_reauth { 1 } else { 0 })
    .execute(pool)
    .await?;
    sqlx::query_as::<_, crate::models::Secret>("SELECT * FROM secrets WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn get_secret(pool: &SqlitePool, id: &str) -> AppResult<crate::models::Secret> {
    sqlx::query_as::<_, crate::models::Secret>("SELECT * FROM secrets WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound {
            entity: "secret",
            id: id.to_string(),
        })
}

pub async fn list_secrets(
    pool: &SqlitePool,
    user_id: &str,
) -> AppResult<Vec<crate::models::Secret>> {
    let rows = sqlx::query_as::<_, crate::models::Secret>(
        "SELECT * FROM secrets WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_secret(
    pool: &SqlitePool,
    id: &str,
    value_enc: &str,
) -> AppResult<crate::models::Secret> {
    sqlx::query("UPDATE secrets SET value_enc = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(value_enc)
        .bind(id)
        .execute(pool)
        .await?;
    get_secret(pool, id).await
}

pub async fn delete_secret(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let r = sqlx::query("DELETE FROM secrets WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "secret",
            id: id.to_string(),
        });
    }
    Ok(())
}

// User profiles
pub async fn get_or_create_user(
    pool: &SqlitePool,
    username: &str,
) -> AppResult<crate::models::UserProfile> {
    if let Ok(Some(user)) = sqlx::query_as::<_, crate::models::UserProfile>(
        "SELECT * FROM user_profiles WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    {
        return Ok(user);
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO user_profiles (id, username) VALUES (?, ?)")
        .bind(&id)
        .bind(username)
        .execute(pool)
        .await?;
    sqlx::query_as::<_, crate::models::UserProfile>("SELECT * FROM user_profiles WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn list_user_profiles(pool: &SqlitePool) -> AppResult<Vec<crate::models::UserProfile>> {
    sqlx::query_as::<_, crate::models::UserProfile>("SELECT * FROM user_profiles ORDER BY username")
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)
}

/// Number of registered users (used to make the first user admin; US-SEC).
pub async fn count_users(pool: &SqlitePool) -> AppResult<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_profiles")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// True when the given user profile is an admin (US-SEC).
pub async fn is_admin(pool: &SqlitePool, user_id: &str) -> AppResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT is_admin FROM user_profiles WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(a,)| a != 0).unwrap_or(false))
}

/// Grant or revoke admin rights (US-SEC).
pub async fn set_admin(pool: &SqlitePool, user_id: &str, admin: bool) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE user_profiles SET is_admin = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(if admin { 1 } else { 0 })
    .bind(user_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "user",
            id: user_id.to_string(),
        });
    }
    Ok(())
}

/// Delete a user; secrets and user keys are removed with them (FK cascade).
/// This is the escape hatch when a password is forgotten (US-SEC).
pub async fn delete_user(pool: &SqlitePool, user_id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM user_profiles WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "user",
            id: user_id.to_string(),
        });
    }
    Ok(())
}

/// Replace a user's password hash (admin reset; US-SEC). The encryption key is
/// NOT re-derivable from the new password, so secrets stay encrypted with the
/// old key — pair with `delete_user` when secrets must be recoverable.
pub async fn set_password_hash(
    pool: &SqlitePool,
    user_id: &str,
    password_hash: &str,
    salt: &str,
) -> AppResult<()> {
    let result = sqlx::query(
        r#"UPDATE user_profiles
           SET password_hash = ?, salt = ?, auth_method = 'password', updated_at = datetime('now')
           WHERE id = ?"#,
    )
    .bind(password_hash)
    .bind(salt)
    .bind(user_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "user",
            id: user_id.to_string(),
        });
    }
    Ok(())
}

// Plugins
pub async fn create_plugin(
    pool: &SqlitePool,
    name: &str,
    version: &str,
    manifest: &str,
) -> AppResult<crate::models::Plugin> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO plugins (id, name, version, manifest) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(version)
        .bind(manifest)
        .execute(pool)
        .await?;
    sqlx::query_as::<_, crate::models::Plugin>("SELECT * FROM plugins WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn list_plugins(pool: &SqlitePool) -> AppResult<Vec<crate::models::Plugin>> {
    sqlx::query_as::<_, crate::models::Plugin>("SELECT * FROM plugins ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn enable_plugin(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("UPDATE plugins SET enabled = 1, updated_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn disable_plugin(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query("UPDATE plugins SET enabled = 0, updated_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// SSH hosts
pub async fn create_ssh_host(
    pool: &SqlitePool,
    name: &str,
    hostname: &str,
    port: i32,
    username: Option<&str>,
    key_path: Option<&str>,
) -> AppResult<crate::models::SshHost> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO ssh_hosts (id, name, hostname, port, username, key_path) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id).bind(name).bind(hostname).bind(port).bind(username).bind(key_path)
    .execute(pool).await?;
    sqlx::query_as::<_, crate::models::SshHost>("SELECT * FROM ssh_hosts WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn list_ssh_hosts(pool: &SqlitePool) -> AppResult<Vec<crate::models::SshHost>> {
    sqlx::query_as::<_, crate::models::SshHost>("SELECT * FROM ssh_hosts ORDER BY name")
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn delete_ssh_host(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let r = sqlx::query("DELETE FROM ssh_hosts WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: "ssh_host",
            id: id.to_string(),
        });
    }
    Ok(())
}

// Scheduled tasks
pub async fn create_scheduled_task(
    pool: &SqlitePool,
    workflow_id: &str,
    cron_expr: &str,
) -> AppResult<crate::models::ScheduledTask> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO scheduled_tasks (id, workflow_id, cron_expr) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(workflow_id)
        .bind(cron_expr)
        .execute(pool)
        .await?;
    sqlx::query_as::<_, crate::models::ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
        .map_err(AppError::Database)
}

pub async fn list_scheduled_tasks(
    pool: &SqlitePool,
) -> AppResult<Vec<crate::models::ScheduledTask>> {
    sqlx::query_as::<_, crate::models::ScheduledTask>(
        "SELECT * FROM scheduled_tasks WHERE enabled = 1 ORDER BY workflow_id",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory SQLite with a single connection (so all queries share the same
    /// schema) and the real migrations applied.
    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    fn cmd_req(name: &str, content: &str) -> CreateEntity {
        CreateEntity {
            name: name.to_string(),
            description: None,
            content: Some(content.to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: Some(vec!["test".to_string()]),
            metadata_json: None,
        }
    }

    // ── Entities (US-CMD-01..09) ────────────────────────────────────────────

    /// Scenario: create, read, update, delete a command entity
    #[tokio::test]
    async fn given_fresh_db_when_entity_crud_then_round_trip_works() {
        let pool = test_pool().await;

        // Create
        let entity = create_entity(&pool, &cmd_req("list files", "ls -la"))
            .await
            .unwrap();
        assert!(!entity.id.is_empty());
        assert_eq!(entity.type_id, "cmd");
        assert_eq!(entity.content.as_deref(), Some("ls -la"));

        // Read
        let fetched = get_entity(&pool, &entity.id).await.unwrap();
        assert_eq!(fetched.name, "list files");

        // Listed under its type
        let listed = list_entities(&pool, Some("cmd"), None).await.unwrap();
        assert_eq!(listed.len(), 1);

        // Update
        let updated = update_entity(&pool, &entity.id, &cmd_req("list files v2", "ls -lah"))
            .await
            .unwrap();
        assert_eq!(updated.name, "list files v2");
        assert_eq!(updated.content.as_deref(), Some("ls -lah"));

        // Delete
        delete_entity(&pool, &entity.id).await.unwrap();
        assert!(get_entity(&pool, &entity.id).await.is_err());
        assert_eq!(
            list_entities(&pool, Some("cmd"), None).await.unwrap().len(),
            0
        );
    }

    /// Scenario: deleting a missing entity is a NotFound error, not a panic
    #[tokio::test]
    async fn given_missing_entity_when_deleted_then_not_found_error() {
        let pool = test_pool().await;
        let err = delete_entity(&pool, "does-not-exist").await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    /// Scenario: tags attached on create are queryable (US-CMD-04, US-SRCH-03)
    #[tokio::test]
    async fn given_entity_with_tags_when_filtered_then_entity_found() {
        let pool = test_pool().await;
        let entity = create_entity(&pool, &cmd_req("docker ps", "docker ps -a"))
            .await
            .unwrap();

        let tags = get_entity_tags(&pool, &entity.id).await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "test");

        let filtered = filter_by_tags(&pool, &["test".to_string()]).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, entity.id);
    }

    /// Scenario: FTS search finds a created entity (US-SRCH-01)
    #[tokio::test]
    async fn given_created_entity_when_fts_searched_then_entity_found() {
        let pool = test_pool().await;
        create_entity(&pool, &cmd_req("list docker containers", "docker ps -a"))
            .await
            .unwrap();

        let results = search_entities(&pool, "docker").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "list docker containers");
    }

    // ── Projects (US-PROJ-01..07) ───────────────────────────────────────────

    /// Scenario: create, update, delete a project
    #[tokio::test]
    async fn given_fresh_db_when_project_crud_then_round_trip_works() {
        let pool = test_pool().await;

        let project = create_project(
            &pool,
            &CreateProject {
                name: "infra".to_string(),
                description: Some("server tooling".to_string()),
            },
        )
        .await
        .unwrap();

        // Update
        let updated = update_project(
            &pool,
            &project.id,
            &CreateProject {
                name: "infra-2".to_string(),
                description: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.name, "infra-2");
        assert_eq!(updated.description, None);

        // Listed
        assert_eq!(list_projects(&pool).await.unwrap().len(), 1);

        // Delete
        delete_project(&pool, &project.id).await.unwrap();
        assert!(list_projects(&pool).await.unwrap().is_empty());
        let err = delete_project(&pool, &project.id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    /// Scenario: entities link to a project and are listed by it
    #[tokio::test]
    async fn given_entity_in_project_when_listed_by_project_then_found() {
        let pool = test_pool().await;
        let project = create_project(
            &pool,
            &CreateProject {
                name: "p1".into(),
                description: None,
            },
        )
        .await
        .unwrap();

        let mut req = cmd_req("in project", "echo hi");
        req.project_id = Some(project.id.clone());
        let entity = create_entity(&pool, &req).await.unwrap();

        let by_project = list_entities_by_project(&pool, &project.id).await.unwrap();
        assert_eq!(by_project.len(), 1);
        assert_eq!(by_project[0].id, entity.id);
    }

    // ── Secrets (US-SEC-02..04) ─────────────────────────────────────────────

    /// Scenario: secrets are stored per user and deleted
    #[tokio::test]
    async fn given_secrets_when_crud_then_per_user_and_deletable() {
        let pool = test_pool().await;
        // secrets.user_id references user_profiles(id) — resolve real profile ids
        let alice = get_or_create_user(&pool, "alice").await.unwrap().id;
        let bob = get_or_create_user(&pool, "bob").await.unwrap().id;

        create_secret(&pool, &alice, "api-key", "enc-a")
            .await
            .unwrap();
        create_secret(&pool, &bob, "api-key", "enc-b")
            .await
            .unwrap();

        // Per-user listing
        let alice_secrets = list_secrets(&pool, &alice).await.unwrap();
        assert_eq!(alice_secrets.len(), 1);
        assert_eq!(alice_secrets[0].value_enc, "enc-a");
        assert_eq!(list_secrets(&pool, &bob).await.unwrap().len(), 1);

        // Update
        let updated = update_secret(&pool, &alice_secrets[0].id, "enc-a2")
            .await
            .unwrap();
        assert_eq!(updated.value_enc, "enc-a2");

        // Delete
        delete_secret(&pool, &alice_secrets[0].id).await.unwrap();
        assert_eq!(list_secrets(&pool, &alice).await.unwrap().len(), 0);
        let err = delete_secret(&pool, &alice_secrets[0].id)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    // ── Workflow runs (US-WF-08) ────────────────────────────────────────────

    /// Scenario: workflow run history is recorded and listable
    #[tokio::test]
    async fn given_workflow_run_when_inserted_then_listable() {
        let pool = test_pool().await;
        // workflow_runs.workflow_id references entities(id)
        let wf = create_entity(
            &pool,
            &CreateEntity {
                name: "nightly".to_string(),
                description: None,
                content: Some("log('hi')".to_string()),
                type_id: "wf".to_string(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        insert_workflow_run(
            &pool,
            &WorkflowRun {
                run_id: "r1".into(),
                workflow_id: wf.id.clone(),
                success: true,
                output: Some("ok".into()),
                error: None,
                duration_ms: Some(42),
                steps_completed: Some(2),
                created_at: now.clone(),
            },
        )
        .await
        .unwrap();
        insert_workflow_run(
            &pool,
            &WorkflowRun {
                run_id: "r2".into(),
                workflow_id: wf.id.clone(),
                success: false,
                output: None,
                error: Some("boom".into()),
                duration_ms: Some(5),
                steps_completed: Some(0),
                created_at: now,
            },
        )
        .await
        .unwrap();

        let runs = list_workflow_runs_by_workflow_id(&pool, &wf.id)
            .await
            .unwrap();
        assert_eq!(runs.len(), 2);
        assert!(runs.iter().any(|r| r.success && r.run_id == "r1"));
        assert!(runs
            .iter()
            .any(|r| !r.success && r.error.as_deref() == Some("boom")));
    }
}
