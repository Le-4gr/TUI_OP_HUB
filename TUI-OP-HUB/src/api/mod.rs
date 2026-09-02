//! API layer (US-API-01, US-API-02, US-API-03).
use crate::models::*;
use crate::repository;
use crate::workflow::{self, WorkflowResult};
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

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
        .route(
            "/entities",
            axum::routing::post(create_entity_handler).get(list_entities_handler),
        )
        .route("/entities/search", get(search_handler))
        .route("/entities/filter-by-tags", get(filter_by_tags_handler))
        .route(
            "/entities/{id}",
            get(get_entity_handler)
                .put(update_entity_handler)
                .delete(delete_entity_handler),
        )
        .route("/entities/{id}/tags", get(get_entity_tags_handler))
        .route(
            "/entities/{id}/run",
            axum::routing::post(run_entity_handler),
        )
        .route(
            "/projects",
            axum::routing::post(create_project_handler).get(list_projects_handler),
        )
        .route(
            "/projects/{id}",
            get(get_project_handler).delete(delete_project_handler),
        )
        .route(
            "/projects/{id}/entities",
            get(list_project_entities_handler),
        )
        .route("/projects/{id}/dashboard", get(project_dashboard_handler))
        .route(
            "/workflows/{id}/execute",
            axum::routing::post(execute_workflow_handler),
        )
        // Cron scheduling (US-WF-07)
        .route(
            "/workflows/{id}/schedule",
            axum::routing::post(schedule_workflow_handler),
        )
        .route("/schedules", axum::routing::get(list_schedules_handler))
        .route(
            "/schedules/{id}",
            axum::routing::delete(delete_schedule_handler),
        )
        // Knowledge-base sharing (US-CMD-01)
        .route("/export", axum::routing::get(export_handler))
        .route("/import", axum::routing::post(import_handler))
        .route("/tags", get(list_tags_handler))
        .route("/types", get(list_types_handler))
        // Developer-mode user management (US-NF): wiped/reset auth state while
        // testing. Only available when running a debug build (cargo run/test)
        // or with TUI_OP_HUB_DEV=1.
        .route(
            "/users",
            get(list_users_handler).delete(delete_all_users_handler),
        )
        // Secrets
        .route(
            "/secrets",
            axum::routing::get(list_secrets_handler).post(create_secret_handler),
        )
        .route(
            "/secrets/{id}",
            get(get_secret_handler)
                .put(update_secret_handler)
                .delete(delete_secret_handler),
        )
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: "0.2.0".into(),
    })
}

async fn create_entity_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateEntity>,
) -> Result<Json<Entity>, String> {
    repository::create_entity(&state.pool, &req)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn list_entities_handler(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Entity>>, String> {
    repository::list_entities(&state.pool, q.type_id.as_deref(), q.project_id.as_deref())
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn get_entity_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Entity>, String> {
    repository::get_entity(&state.pool, &id)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn update_entity_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateEntity>,
) -> Result<Json<Entity>, String> {
    repository::update_entity(&state.pool, &id, &req)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn delete_entity_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, String> {
    repository::delete_entity(&state.pool, &id)
        .await
        .map(|_| "deleted".to_string())
        .map_err(|e| e.to_string())
}

async fn search_handler(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, String> {
    repository::search_entities(&state.pool, &q.q)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn filter_by_tags_handler(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<Entity>>, String> {
    let tags_str = params.get("tags").cloned().unwrap_or_default();
    let tags: Vec<String> = tags_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    repository::filter_by_tags(&state.pool, &tags)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn get_entity_tags_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Tag>>, String> {
    repository::get_entity_tags(&state.pool, &id)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

/// Trigger a workflow/command execution via the API (US-API-03).
///
/// Looks up the entity by ID, retrieves its `content` field, and executes
/// it via `$SHELL -c`. Returns stdout, stderr, and exit code.
async fn run_entity_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunResponse>, String> {
    let entity = repository::get_entity(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())?;

    let content = entity
        .content
        .as_deref()
        .ok_or_else(|| format!("entity {} has no content to execute", id))?;

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

async fn create_project_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateProject>,
) -> Result<Json<Project>, String> {
    repository::create_project(&state.pool, &req)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn list_projects_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<Project>>, String> {
    repository::list_projects(&state.pool)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn get_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Project>, String> {
    repository::get_project(&state.pool, &id)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn delete_project_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, String> {
    repository::delete_project(&state.pool, &id)
        .await
        .map(|_| "deleted".to_string())
        .map_err(|e| e.to_string())
}

async fn list_project_entities_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Entity>>, String> {
    repository::list_entities_by_project(&state.pool, &id)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
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

async fn project_dashboard_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProjectDashboard>, String> {
    let project = repository::get_project(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())?;
    let entities = repository::list_entities_by_project(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())?;

    let total = entities.len();
    let mut type_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
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
    repository::list_tags(&state.pool)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn list_types_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<EntityType>>, String> {
    repository::list_types(&state.pool)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

// ── Developer-mode user management (US-NF) ──────────────────────────────────

async fn list_users_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, String> {
    if !crate::auth::dev_mode_enabled() {
        return Err(
            "user management requires developer mode (debug build or TUI_OP_HUB_DEV=1)".to_string(),
        );
    }
    repository::list_user_profiles(&state.pool)
        .await
        .map(|users| Json(serde_json::json!({ "users": users, "count": users.len() })))
        .map_err(|e| e.to_string())
}

/// DEV: delete every user (secrets/user keys cascade). Without logging in —
/// this endpoint only exists in developer mode so auth state can be reset
/// between tests.
async fn delete_all_users_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, String> {
    if !crate::auth::dev_mode_enabled() {
        return Err(
            "user management requires developer mode (debug build or TUI_OP_HUB_DEV=1)".to_string(),
        );
    }
    repository::delete_all_users(&state.pool)
        .await
        .map(|deleted| {
            Json(serde_json::json!({
                "deleted": deleted,
                "note": "next signup becomes admin"
            }))
        })
        .map_err(|e| e.to_string())
}

// Secrets handlers
#[derive(Deserialize)]
pub struct CreateSecretReq {
    pub name: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct UpdateSecretReq {
    pub value: String,
}

/// Resolve the API's default user to a `user_profiles.id` (creating the profile
/// row if needed). Secrets are keyed by profile id, not by username.
async fn default_user_id(pool: &sqlx::SqlitePool) -> Result<String, String> {
    repository::get_or_create_user(pool, "default")
        .await
        .map(|p| p.id)
        .map_err(|e| e.to_string())
}

async fn list_secrets_handler(State(state): State<AppState>) -> Result<Json<Vec<Secret>>, String> {
    let user_id = default_user_id(&state.pool).await?;
    repository::list_secrets(&state.pool, &user_id)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn create_secret_handler(
    State(state): State<AppState>,
    Json(req): Json<CreateSecretReq>,
) -> Result<Json<Secret>, String> {
    let user_id = default_user_id(&state.pool).await?;
    // encrypt value server-side
    match crate::secrets::encrypt_for_user(&state.pool, &user_id, &req.value).await {
        Ok(enc) => repository::create_secret(&state.pool, &user_id, &req.name, &enc)
            .await
            .map(Json)
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

async fn get_secret_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    let s = repository::get_secret(&state.pool, &id)
        .await
        .map_err(|e| e.to_string())?;
    // decrypt before returning
    match crate::secrets::decrypt_for_user_id(&state.pool, &s.user_id, &s.value_enc).await {
        Ok(val) => Ok(Json(
            serde_json::json!({"id": s.id, "name": s.name, "value": val}),
        )),
        Err(e) => Err(e.to_string()),
    }
}

async fn update_secret_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSecretReq>,
) -> Result<Json<Secret>, String> {
    match crate::secrets::encrypt_for_user(&state.pool, "default", &req.value).await {
        Ok(enc) => repository::update_secret(&state.pool, &id, &enc)
            .await
            .map(Json)
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

async fn delete_secret_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, String> {
    repository::delete_secret(&state.pool, &id)
        .await
        .map(|_| "deleted".to_string())
        .map_err(|e| e.to_string())
}

/// Execute a workflow via the API (US-WF-06, US-API-03).
#[derive(Deserialize)]
pub struct ExecuteWorkflowRequest {
    pub variables: Option<std::collections::HashMap<String, String>>,
}

async fn execute_workflow_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<WorkflowResult>, String> {
    let exec_pool = state.pool.clone();
    let persist_pool = state.pool.clone();
    let variables = req.variables;
    // clone id for use after moving into spawn_blocking
    let id_for_exec = id.clone();

    // Use spawn_blocking because Lua is not Send by default
    let result = tokio::task::spawn_blocking(move || {
        // Create a runtime for the blocking task
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            workflow::execute_workflow_by_id(exec_pool, &id_for_exec, variables).await
        })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())?;

    // Persist run result to DB (best-effort)
    let run_record = crate::models::WorkflowRun {
        run_id: result.run_id.clone(),
        workflow_id: id.clone(),
        success: result.success,
        output: Some(result.output.clone()),
        error: result.error.clone(),
        duration_ms: Some(result.duration_ms as i64),
        steps_completed: Some(result.steps_completed as i32),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    if let Err(e) = repository::insert_workflow_run(&persist_pool, &run_record).await {
        tracing::warn!(error = %e, "failed to persist workflow run");
    }

    Ok(Json(result))
}

// ── Cron scheduling (US-WF-07) ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct ScheduleRequest {
    cron_expr: String,
}

/// Schedule a workflow on a cron expression. The expression is validated
/// before persisting; the scheduler daemon picks it up on its next poll
/// (or at startup).
async fn schedule_workflow_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ScheduleRequest>,
) -> Result<Json<crate::models::ScheduledTask>, String> {
    // Fail fast on invalid expressions so clients get a useful error,
    // and store the normalized form so the daemon can reload it.
    let (normalized, _) = match crate::scheduler::validate_cron(&req.cron_expr) {
        Ok(v) => v,
        Err(e) => return Err(e.to_string()),
    };
    repository::create_scheduled_task(&state.pool, &id, &normalized)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn list_schedules_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::ScheduledTask>>, String> {
    repository::list_scheduled_tasks(&state.pool)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

async fn delete_schedule_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    repository::delete_scheduled_task(&state.pool, &id)
        .await
        .map(|_| Json(serde_json::json!({"deleted": id})))
        .map_err(|e| e.to_string())
}

// ── Knowledge-base sharing (US-CMD-01) ─────────────────────────────────────

/// Export the knowledge base as a portable JSON bundle. Secrets are always
/// **excluded** on this endpoint (use the TUI for passphrase-encrypted or
/// explicit plaintext exports).
async fn export_handler(
    State(state): State<AppState>,
) -> Result<Json<crate::share::KnowledgeBundle>, String> {
    crate::share::export_knowledge(&state.pool, None, crate::share::SecretMode::Exclude, None)
        .await
        .map(Json)
        .map_err(|e| e.to_string())
}

/// Import a knowledge bundle: merges by (name, type); existing entries win.
/// Accepts lenient shapes: a full bundle, a bare array of entities, or an
/// object with only an `entities` array (see `share::bundle_from_json`).
async fn import_handler(
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, String> {
    let text =
        String::from_utf8(body.to_vec()).map_err(|_| "body is not valid UTF-8".to_string())?;
    let bundle = crate::share::bundle_from_json(&text).map_err(|e| e.to_string())?;
    let (imported, skipped) = crate::share::import_knowledge(&state.pool, &bundle)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Json(
        serde_json::json!({ "imported": imported, "skipped": skipped }),
    ))
}
