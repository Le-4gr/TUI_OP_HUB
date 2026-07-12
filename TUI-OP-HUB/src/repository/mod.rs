//! Repository layer (US-CMD-01..09, US-PROJ-01..06, US-SRCH-01..04).
use sqlx::SqlitePool;
use uuid::Uuid;
use crate::error::{AppError, AppResult};
use crate::models::*;

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
                .bind(&tag_id).bind(tag_name).execute(pool).await?;
            sqlx::query("INSERT OR IGNORE INTO entity_tags (entity_id, tag_id) SELECT ?, id FROM tags WHERE name = ?")
                .bind(&id).bind(tag_name).execute(pool).await?;
        }
    }
    get_entity(pool, &id).await
}

pub async fn get_entity(pool: &SqlitePool, id: &str) -> AppResult<Entity> {
    sqlx::query_as::<_, Entity>("SELECT * FROM entities WHERE id = ?")
        .bind(id)
        .fetch_optional(pool).await?
        .ok_or(AppError::NotFound { entity: "entity", id: id.to_string() })
}

pub async fn list_entities(pool: &SqlitePool, type_filter: Option<&str>, project_id: Option<&str>) -> AppResult<Vec<Entity>> {
    let rows = sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE (? IS NULL OR type_id = ?) AND (? IS NULL OR project_id = ?) ORDER BY created_at DESC"
    )
    .bind(type_filter).bind(type_filter)
    .bind(project_id).bind(project_id)
    .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_entities_by_project(pool: &SqlitePool, project_id: &str) -> AppResult<Vec<Entity>> {
    let rows = sqlx::query_as::<_, Entity>(
        "SELECT * FROM entities WHERE project_id = ? ORDER BY created_at DESC"
    )
    .bind(project_id)
    .fetch_all(pool).await?;
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
    let result = sqlx::query("DELETE FROM entities WHERE id = ?").bind(id).execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound { entity: "entity", id: id.to_string() });
    }
    Ok(())
}

pub async fn create_project(pool: &SqlitePool, req: &CreateProject) -> AppResult<Project> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO projects (id, name, description) VALUES (?, ?, ?)")
        .bind(&id).bind(&req.name).bind(&req.description)
        .execute(pool).await?;
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(&id).fetch_one(pool).await.map_err(AppError::Database)
}

pub async fn get_project(pool: &SqlitePool, id: &str) -> AppResult<Project> {
    sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool).await?
        .ok_or(AppError::NotFound { entity: "project", id: id.to_string() })
}

pub async fn list_projects(pool: &SqlitePool) -> AppResult<Vec<Project>> {
    let rows = sqlx::query_as::<_, Project>("SELECT * FROM projects ORDER BY created_at DESC")
        .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_project(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?").bind(id).execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound { entity: "project", id: id.to_string() });
    }
    Ok(())
}

pub async fn list_tags(pool: &SqlitePool) -> AppResult<Vec<Tag>> {
    let rows = sqlx::query_as::<_, Tag>("SELECT * FROM tags ORDER BY name")
        .fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_types(pool: &SqlitePool) -> AppResult<Vec<EntityType>> {
    let rows = sqlx::query_as::<_, EntityType>("SELECT * FROM types ORDER BY name")
        .fetch_all(pool).await?;
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
