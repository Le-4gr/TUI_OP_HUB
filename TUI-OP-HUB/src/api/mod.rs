//! API layer (US-API-01, US-API-02, US-API-03).
use axum::{extract::{Path, Query, State}, response::Json, routing::get, Router};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use crate::models::*;
use crate::repository;

#[derive(Clone)]
pub struct AppState {
    pub pool: Arc<SqlitePool>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub type_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

/// Response for workflow/command execution (US-API-03).
#[derive(Serialize)]
pub struct RunResponse {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub fn router(pool: Arc<SqlitePool>) -> Router {
    let state = AppState { pool };
    Router::new()
        .route("/health", get(health))
        .route("/entities", axum::routing::post(create_entity_handler).get(list_entities_handler))
        .route("/entities/search", get(search_handler))
        .route("/entities/filter-by-tags", get(filter_by_tags_handler))
        .route("/entities/{id}", get(get_entity_handler).put(update_entity_handler).delete(delete_entity_handler))
        .route("/entities/{id}/tags", get(get_entity_tags_handler))
        .route("/entities/{id}/run", axum::routing::post(run_entity_handler))
        .route("/projects", axum::routing::post(create_project_handler).get(list_projects_handler))
        .route("/projects/{id}", get(get_project_handler).delete(delete_project_handler))
        .route("/projects/{id}/entities", get(list_project_entities_handler))
        .route("/projects/{id}/dashboard", get(project_dashboard_handler))
        .route("/tags", get(list_tags_handler))
        .route("/types", get(list_types_handler))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), version: "0.2.0".into() })
}

async fn create_entity_handler(State(state): State<AppState>, Json(req): Json<CreateEntity>) -> Result<Json<Entity>, String> {
    repository::create_entity(&state.pool, &req).await
        .map(Json).map_err(|e| e.to_string())
}

async fn list_entities_handler(State(state): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<Entity>>, String> {
    repository::list_entities(&state.pool, q.type_id.as_deref(), q.project_id.as_deref()).await
        .map(Json).map_err(|e| e.to_string())
}

async fn get_entity_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Entity>, String> {
    repository::get_entity(&state.pool, &id).await
        .map(Json).map_err(|e| e.to_string())
}

async fn update_entity_handler(State(state): State<AppState>, Path(id): Path<String>, Json(req): Json<CreateEntity>) -> Result<Json<Entity>, String> {
    repository::update_entity(&state.pool, &id, &req).await
        .map(Json).map_err(|e| e.to_string())
}

async fn delete_entity_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<String, String> {
    repository::delete_entity(&state.pool, &id).await
        .map(|_| "deleted".to_string()).map_err(|e| e.to_string())
}

async fn search_handler(State(state): State<AppState>, Query(q): Query<SearchQuery>) -> Result<Json<Vec<SearchResult>>, String> {
    repository::search_entities(&state.pool, &q.q).await
        .map(Json).map_err(|e| e.to_string())
}

async fn filter_by_tags_handler(State(state): State<AppState>, Query(params): Query<std::collections::HashMap<String, String>>) -> Result<Json<Vec<Entity>>, String> {
    let tags_str = params.get("tags").cloned().unwrap_or_default();
    let tags: Vec<String> = tags_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    repository::filter_by_tags(&state.pool, &tags).await
        .map(Json).map_err(|e| e.to_string())
}

async fn get_entity_tags_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Vec<Tag>>, String> {
    repository::get_entity_tags(&state.pool, &id).await
        .map(Json).map_err(|e| e.to_string())
}

/// Trigger a workflow/command execution via the API (US-API-03).
///
/// Looks up the entity by ID, retrieves its `content` field, and executes
/// it via `$SHELL -c`. Returns stdout, stderr, and exit code.
async fn run_entity_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<RunResponse>, String> {
    let entity = repository::get_entity(&state.pool, &id).await
        .map_err(|e| e.to_string())?;

    let content = entity.content.as_deref().ok_or_else(|| {
        format!("entity {} has no content to execute", id)
    })?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
    let output = std::process::Command::new(&shell)
        .arg("-c")
        .arg(content)
        .output()
        .map_err(|e| format!("failed to execute: {e}"))?;

    Ok(Json(RunResponse {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }))
}

async fn create_project_handler(State(state): State<AppState>, Json(req): Json<CreateProject>) -> Result<Json<Project>, String> {
    repository::create_project(&state.pool, &req).await
        .map(Json).map_err(|e| e.to_string())
}

async fn list_projects_handler(State(state): State<AppState>) -> Result<Json<Vec<Project>>, String> {
    repository::list_projects(&state.pool).await
        .map(Json).map_err(|e| e.to_string())
}

async fn get_project_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Project>, String> {
    repository::get_project(&state.pool, &id).await
        .map(Json).map_err(|e| e.to_string())
}

async fn delete_project_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<String, String> {
    repository::delete_project(&state.pool, &id).await
        .map(|_| "deleted".to_string()).map_err(|e| e.to_string())
}

async fn list_project_entities_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<Vec<Entity>>, String> {
    repository::list_entities_by_project(&state.pool, &id).await
        .map(Json).map_err(|e| e.to_string())
}

/// Project dashboard endpoint (US-PROJ-07).
///
/// Returns a summary of the project's resources grouped by type.
#[derive(Serialize)]
pub struct ProjectDashboard {
    pub project: Project,
    pub total_entities: usize,
    pub by_type: Vec<(String, usize)>,
    pub recent_entities: Vec<Entity>,
}

async fn project_dashboard_handler(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<ProjectDashboard>, String> {
    let project = repository::get_project(&state.pool, &id).await
        .map_err(|e| e.to_string())?;
    let entities = repository::list_entities_by_project(&state.pool, &id).await
        .map_err(|e| e.to_string())?;

    let total = entities.len();
    let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &entities {
        *type_counts.entry(e.type_id.clone()).or_insert(0) += 1;
    }
    let mut by_type: Vec<(String, usize)> = type_counts.into_iter().collect();
    by_type.sort_by(|a, b| b.1.cmp(&a.1));

    let recent = entities.into_iter().take(10).collect();

    Ok(Json(ProjectDashboard {
        project,
        total_entities: total,
        by_type,
        recent_entities: recent,
    }))
}

async fn list_tags_handler(State(state): State<AppState>) -> Result<Json<Vec<Tag>>, String> {
    repository::list_tags(&state.pool).await
        .map(Json).map_err(|e| e.to_string())
}

async fn list_types_handler(State(state): State<AppState>) -> Result<Json<Vec<EntityType>>, String> {
    repository::list_types(&state.pool).await
        .map(Json).map_err(|e| e.to_string())
}