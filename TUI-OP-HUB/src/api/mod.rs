//! API layer (US-API-01, US-API-02).
use axum::{extract::{Path, Query, State}, response::Json, routing::{get, post, delete, put}, Router};
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

pub fn router(pool: Arc<SqlitePool>) -> Router {
    let state = AppState { pool };
    Router::new()
        .route("/health", get(health))
        .route("/entities", post(create_entity_handler).get(list_entities_handler))
        .route("/entities/{id}", get(get_entity_handler).put(update_entity_handler).delete(delete_entity_handler))
        .route("/entities/search", get(search_handler))
        .route("/projects", post(create_project_handler).get(list_projects_handler))
        .route("/tags", get(list_tags_handler))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into(), version: "0.1.0".into() })
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

async fn search_handler(State(state): State<AppState>, Query(params): Query<std::collections::HashMap<String, String>>) -> Result<Json<Vec<SearchResult>>, String> {
    let q = params.get("q").cloned().unwrap_or_default();
    repository::search_entities(&state.pool, &q).await
        .map(Json).map_err(|e| e.to_string())
}

async fn create_project_handler(State(state): State<AppState>, Json(req): Json<CreateProject>) -> Result<Json<Project>, String> {
    repository::create_project(&state.pool, &req).await
        .map(Json).map_err(|e| e.to_string())
}

async fn list_projects_handler(State(state): State<AppState>) -> Result<Json<Vec<Project>>, String> {
    repository::list_projects(&state.pool).await
        .map(Json).map_err(|e| e.to_string())
}

async fn list_tags_handler(State(state): State<AppState>) -> Result<Json<Vec<Tag>>, String> {
    repository::list_tags(&state.pool).await
        .map(Json).map_err(|e| e.to_string())
}
