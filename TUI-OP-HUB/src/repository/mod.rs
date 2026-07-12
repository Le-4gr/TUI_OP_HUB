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
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO secrets (id, user_id, name, value_enc) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(name)
    .bind(value_enc)
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

pub async fn list_secrets(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<crate::models::Secret>> {
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

// Plugins
pub async fn create_plugin(
    pool: &SqlitePool,
    name: &str,
    version: &str,
    manifest: &str,
) -> AppResult<crate::models::Plugin> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO plugins (id, name, version, manifest) VALUES (?, ?, ?, ?)",
    )
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
        .bind(&id).fetch_one(pool).await.map_err(AppError::Database)
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

pub async fn list_scheduled_tasks(pool: &SqlitePool) -> AppResult<Vec<crate::models::ScheduledTask>> {
    sqlx::query_as::<_, crate::models::ScheduledTask>(
        "SELECT * FROM scheduled_tasks WHERE enabled = 1 ORDER BY workflow_id",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)
}
