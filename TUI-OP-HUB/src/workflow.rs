//! Workflow execution engine (US-WF-01..09, US-PLG-02, US-API-04).
//!
//! Provides Lua-based workflow execution with safe host functions.
//! Supports linear workflows and basic DAG execution.

use mlua::Lua;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::Entity;
use crate::repository;

/// Workflow execution context.
#[derive(Debug, Clone)]
pub struct WorkflowContext {
    pub workflow_id: String,
    pub run_id: String,
    pub variables: HashMap<String, String>,
    pub pool: Arc<SqlitePool>,
    /// Profile id of the running user; when set, the user's secrets are
    /// exposed to the script as `secrets.<name>` / `get_secret(name)`
    /// (reauth-protected secrets are excluded; US-SEC, US-WF-04).
    pub user_id: Option<String>,
    /// Cooperative cancellation flag (US-WF-09): checked before every step.
    /// Set through [`cancel_run`] or by sharing this token directly.
    pub cancel: Arc<AtomicBool>,
}

/// Workflow execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub run_id: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub steps_completed: usize,
}

/// Workflow step definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub name: String,
    pub script: String,
    pub depends_on: Vec<String>,
    /// Skip this step unless the Lua expression is truthy (US-FUT-07).
    /// The expression may read `results["<step>"]` and `ctx`.
    #[serde(default)]
    pub run_when: Option<String>,
}

/// Workflow definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<WorkflowStep>,
    pub variables: HashMap<String, String>,
}

impl WorkflowDefinition {
    /// Parse workflow definition from entity content.
    pub fn from_entity(entity: &Entity) -> AppResult<Self> {
        let content = entity
            .content
            .as_ref()
            .ok_or_else(|| AppError::Validation("Workflow entity has no content".to_string()))?;

        // Try to parse as JSON first, then fall back to simple Lua script
        if let Ok(def) = serde_json::from_str::<WorkflowDefinition>(content) {
            Ok(def)
        } else {
            // Create a simple single-step workflow from Lua script
            Ok(WorkflowDefinition {
                name: entity.name.clone(),
                description: entity.description.clone(),
                steps: vec![WorkflowStep {
                    name: "main".to_string(),
                    script: content.clone(),
                    depends_on: vec![],
                    run_when: None,
                }],
                variables: HashMap::new(),
            })
        }
    }
}

/// Workflow execution engine.
pub struct WorkflowEngine {
    lua: Lua,
    _pool: Arc<SqlitePool>,
}

impl WorkflowEngine {
    /// Create a new workflow engine with safe host functions.
    pub fn new(pool: Arc<SqlitePool>) -> AppResult<Self> {
        let lua = Lua::new();

        // Set up safe host functions (US-API-04)
        Self::setup_host_functions(&lua, pool.clone())?;

        Ok(Self { lua, _pool: pool })
    }

    /// Set up safe host functions for Lua scripts.
    fn setup_host_functions(lua: &Lua, _pool: Arc<SqlitePool>) -> AppResult<()> {
        let globals = lua.globals();

        // Host function: run_command (simplified synchronous version)
        let run_command = lua.create_function(|lua, command: String| {
            tracing::info!(command = %command, "workflow executing command");

            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("Command failed: {}", e)))?;

            let result = lua.create_table()?;
            result.set("success", output.status.success())?;
            result.set("exit_code", output.status.code().unwrap_or(-1))?;
            result.set(
                "stdout",
                String::from_utf8_lossy(&output.stdout).to_string(),
            )?;
            result.set(
                "stderr",
                String::from_utf8_lossy(&output.stderr).to_string(),
            )?;

            Ok(result)
        })?;
        globals.set("run_command", run_command)?;

        // Host function: query_entity (simplified - returns basic info)
        let query_entity = lua.create_function(|lua, id: String| {
            // For now, return a placeholder - in a real implementation this would
            // need to be async or use a different approach
            let result = lua.create_table()?;
            result.set("id", id)?;
            result.set("name", "placeholder")?;
            result.set("description", "")?;
            result.set("content", "")?;
            result.set("type_id", "unknown")?;
            Ok(result)
        })?;
        globals.set("query_entity", query_entity)?;

        // Host function: emit_event (placeholder for future event system)
        let emit_event = lua.create_function(|_lua, (event_type, data): (String, String)| {
            tracing::info!(event_type = %event_type, data = %data, "workflow event emitted");
            Ok(())
        })?;
        globals.set("emit_event", emit_event)?;

        // Host function: log
        let log = lua.create_function(|_lua, message: String| {
            tracing::info!(workflow_log = %message);
            Ok(())
        })?;
        globals.set("log", log)?;

        Ok(())
    }

    /// Execute a workflow definition.
    pub async fn execute_workflow(
        &self,
        definition: &WorkflowDefinition,
        context: WorkflowContext,
    ) -> AppResult<WorkflowResult> {
        let start_time = std::time::Instant::now();
        let mut output = String::new();
        let mut steps_completed = 0;

        // Make this run cancellable via `cancel_run(run_id)` (US-WF-09)
        register_run(&context.run_id, context.cancel.clone());

        tracing::info!(
            workflow = %definition.name,
            run_id = %context.run_id,
            "starting workflow execution"
        );

        // Set up workflow context in Lua
        let globals = self.lua.globals();
        let ctx_table = self.lua.create_table()?;
        ctx_table.set("workflow_id", context.workflow_id.clone())?;
        ctx_table.set("run_id", context.run_id.clone())?;

        // Set variables
        let vars_table = self.lua.create_table()?;
        for (key, value) in &definition.variables {
            vars_table.set(key.clone(), value.clone())?;
        }
        for (key, value) in &context.variables {
            vars_table.set(key.clone(), value.clone())?;
        }
        ctx_table.set("vars", vars_table)?;
        globals.set("ctx", ctx_table)?;

        // Expose the running user's secrets as variables (US-SEC-02, US-WF-04):
        //   secrets.<name> or get_secret("<name>")
        // Secrets flagged `requires_reauth` are never exposed automatically.
        if let Some(user_id) = &context.user_id {
            match load_user_secrets(&self.lua, &context.pool, user_id).await {
                Ok(secrets_table) => globals.set("secrets", secrets_table)?,
                Err(e) => tracing::warn!(error = %e, "could not load secrets for workflow"),
            }
        }

        // Execute steps in dependency order (simple linear execution for now).
        // Before every step the cooperative cancel flag is checked (US-WF-09).
        // Each step's return value is captured into the global `results`
        // table so logic nodes can reference prior outputs (US-FUT-07):
        //   results["<step name>"]
        let results_table = self.lua.create_table()?;
        globals.set("results", results_table.clone())?;

        for step in &definition.steps {
            if context.cancel.load(Ordering::Relaxed) {
                let msg = format!("✗ Run cancelled by user after {} step(s)", steps_completed);
                tracing::info!(run_id = %context.run_id, "workflow cancelled by user");
                unregister_run(&context.run_id);
                return Ok(WorkflowResult {
                    run_id: context.run_id,
                    success: false,
                    output,
                    error: Some(msg),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    steps_completed,
                });
            }

            // `run_when` gate (US-FUT-07): skip the step unless the Lua
            // expression is truthy. A broken condition fails the run.
            if let Some(cond) = &step.run_when {
                match self.lua.load(cond.clone()).eval::<mlua::Value>() {
                    Ok(v) => {
                        let truthy = !matches!(v, mlua::Value::Nil | mlua::Value::Boolean(false));
                        if !truthy {
                            output.push_str(&format!(
                                "◌ Step '{}' skipped (run_when is false)\n",
                                step.name
                            ));
                            results_table.set(step.name.as_str(), false)?;
                            continue;
                        }
                    }
                    Err(e) => {
                        let error_msg =
                            format!("✗ Step '{}' run_when condition failed: {}", step.name, e);
                        tracing::error!(step = %step.name, error = %e, "run_when failed");
                        unregister_run(&context.run_id);
                        return Ok(WorkflowResult {
                            run_id: context.run_id,
                            success: false,
                            output,
                            error: Some(error_msg),
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            steps_completed,
                        });
                    }
                }
            }

            tracing::info!(step = %step.name, "executing workflow step");

            match self.lua.load(&step.script).eval::<mlua::Value>() {
                Ok(value) => {
                    steps_completed += 1;
                    // Capture the step result (nil/false → false) so later
                    // logic nodes / run_when gates can reference it.
                    let stored: mlua::Value = match &value {
                        mlua::Value::Nil => mlua::Value::Boolean(false),
                        other => other.clone(),
                    };
                    if let Err(e) = results_table.set(step.name.as_str(), stored) {
                        tracing::warn!(step = %step.name, error = %e, "could not store result");
                    }
                    output.push_str(&format!("✓ Step '{}' completed\n", step.name));
                }
                Err(e) => {
                    let error_msg = format!("✗ Step '{}' failed: {}", step.name, e);
                    tracing::error!(step = %step.name, error = %e, "workflow step failed");

                    unregister_run(&context.run_id);
                    return Ok(WorkflowResult {
                        run_id: context.run_id,
                        success: false,
                        output,
                        error: Some(error_msg),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        steps_completed,
                    });
                }
            }
        }

        let duration = start_time.elapsed().as_millis() as u64;
        tracing::info!(
            workflow = %definition.name,
            run_id = %context.run_id,
            duration_ms = duration,
            "workflow execution completed successfully"
        );

        unregister_run(&context.run_id);
        Ok(WorkflowResult {
            run_id: context.run_id,
            success: true,
            output,
            error: None,
            duration_ms: duration,
            steps_completed,
        })
    }

    /// Execute a simple Lua script (for single commands).
    pub async fn execute_script(
        &self,
        script: &str,
        context: WorkflowContext,
    ) -> AppResult<WorkflowResult> {
        let definition = WorkflowDefinition {
            name: "script".to_string(),
            description: None,
            steps: vec![WorkflowStep {
                name: "main".to_string(),
                script: script.to_string(),
                depends_on: vec![],
                run_when: None,
            }],
            variables: HashMap::new(),
        };

        self.execute_workflow(&definition, context).await
    }
}

/// Create a new workflow execution context.
pub fn create_workflow_context(
    workflow_id: String,
    pool: Arc<SqlitePool>,
    variables: Option<HashMap<String, String>>,
) -> WorkflowContext {
    WorkflowContext {
        workflow_id,
        run_id: Uuid::new_v4().to_string(),
        variables: variables.unwrap_or_default(),
        pool,
        user_id: None,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// Create a context that can access the given user's secrets (US-SEC-02).
pub fn create_workflow_context_for_user(
    workflow_id: String,
    pool: Arc<SqlitePool>,
    variables: Option<HashMap<String, String>>,
    user_id: Option<String>,
) -> WorkflowContext {
    let mut ctx = create_workflow_context(workflow_id, pool, variables);
    ctx.user_id = user_id;
    ctx
}

/// Decrypt the user's secrets into a Lua `secrets` table and install a
/// `get_secret(name)` accessor in the globals. Secrets marked
/// `requires_reauth` are skipped (they need the login password again; US-SEC-05).
async fn load_user_secrets(
    lua: &Lua,
    pool: &Arc<SqlitePool>,
    user_id: &str,
) -> AppResult<mlua::Table> {
    let stored = repository::list_secrets(pool, user_id).await?;
    let table = lua.create_table()?;
    let mut lookup: HashMap<String, String> = HashMap::new();
    for secret in stored {
        if secret.requires_reauth {
            tracing::info!(name = %secret.name, "skipping reauth-protected secret");
            continue;
        }
        match crate::secrets::decrypt_for_user(pool, user_id, &secret.value_enc).await {
            Ok(value) => {
                table.set(secret.name.clone(), value.clone())?;
                lookup.insert(secret.name, value);
            }
            Err(e) => {
                tracing::warn!(name = %secret.name, error = %e, "secret undecryptable, skipping")
            }
        }
    }
    // get_secret("<name>") accessor over the pre-decrypted secrets
    let get_secret = lua.create_function(move |_lua, name: String| {
        Ok(lookup.get(&name).cloned().unwrap_or_default())
    })?;
    lua.globals().set("get_secret", get_secret)?;
    Ok(table)
}

/// How the TUI / API should execute an entity (US-CMD-09, US-ENV-08).
///
/// Distinguishes the base entity types:
/// - `cmd` — one shell command, run synchronously with captured output
/// - `script` — multi-line script: run from a backing file (metadata
///   `{"file": "…"}`) via its language interpreter (python3, lua, node, sh, …)
/// - `app` — a long-running/GUI application, spawned detached
#[derive(Debug, Clone, PartialEq)]
pub enum RunPlan {
    /// `sh -c <command>` with captured output
    Shell(String),
    /// Run `program args…` with captured output (language interpreters)
    Interpreter { program: String, args: Vec<String> },
    /// Launch detached (GUI apps / long-running processes)
    App(String),
}

/// Interpreter for a file path, chosen by extension (US-ENV, scripts in
/// the language of choice).
fn interpreter_for_path(path: &str) -> Option<(&'static str, Vec<String>)> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "py" | "pyw" => Some(("python3", vec![path.to_string()])),
        "lua" => Some(("lua", vec![path.to_string()])),
        "js" | "mjs" => Some(("node", vec![path.to_string()])),
        "rb" => Some(("ruby", vec![path.to_string()])),
        "pl" => Some(("perl", vec![path.to_string()])),
        "sh" | "bash" => Some(("sh", vec![path.to_string()])),
        _ => None,
    }
}

/// Extract `{"file": "…"}` from the entity metadata (file-backed scripts and
/// workflows; US-CMD-05).
pub fn metadata_file(entity: &Entity) -> Option<String> {
    let meta = entity.metadata_json.as_ref()?;
    let value: serde_json::Value = serde_json::from_str(md_as_str(&meta)).ok()?;
    value.get("file")?.as_str().map(|s| s.to_string())
}

fn md_as_str(s: &str) -> &str {
    s
}

/// Build the execution plan for a stored entity (US-CMD-09):
/// - file-backed entities (metadata `{"file": "…"}`) run through their
///   language interpreter
/// - `cmd` runs as a shell command
/// - `script` runs via the interpreter from its shebang (python/sh/lua), or as
///   shell text
/// - `app` is spawned detached (no captured output)
/// - `wf` returns `None` — workflows go through the engine instead
pub fn build_run_plan(entity: &Entity) -> Option<RunPlan> {
    // 1. File-backed entities run from their file, in any language
    if let Some(path) = metadata_file(entity) {
        if let Some((program, args)) = interpreter_for_path(&path) {
            return Some(RunPlan::Interpreter {
                program: program.to_string(),
                args,
            });
        }
        return Some(RunPlan::Shell(format!("sh {}", path)));
    }

    let content = entity.content.as_ref()?;
    match entity.type_id.as_str() {
        "app" => Some(RunPlan::App(content.clone())),
        "script" => {
            // Shebang-aware: run python scripts with python3, others via shell
            if content.trim_start().starts_with("#!") {
                let first = content.lines().next().unwrap_or("");
                if first.contains("python") {
                    return Some(RunPlan::Interpreter {
                        program: "python3".to_string(),
                        args: vec!["-c".to_string(), content.clone()],
                    });
                }
            }
            Some(RunPlan::Shell(content.clone()))
        }
        "cmd" | "script_" | "" => Some(RunPlan::Shell(content.clone())),
        // workflows and other typed entities are not "run" this way
        _ => None,
    }
}

/// True when `path` looks like a JSON workflow definition file (US-WF-03).
pub fn is_json_path(path: &str) -> bool {
    path.to_lowercase().ends_with(".json")
}

/// Execute a workflow entity by ID.
pub async fn execute_workflow_by_id(
    pool: Arc<SqlitePool>,
    entity_id: &str,
    variables: Option<HashMap<String, String>>,
) -> AppResult<WorkflowResult> {
    let entity = repository::get_entity(&pool, entity_id).await?;

    if entity.type_id != "wf" {
        return Err(AppError::Validation(format!(
            "Entity {} is not a workflow (type: {})",
            entity_id, entity.type_id
        )));
    }

    let definition = resolve_workflow_definition(&entity)?;
    let context = create_workflow_context(entity.id.clone(), pool.clone(), variables);
    let engine = WorkflowEngine::new(pool)?;

    engine.execute_workflow(&definition, context).await
}

/// Execute a workflow entity by ID, exposing the user's secrets as variables.
pub async fn execute_workflow_by_id_for_user(
    pool: Arc<SqlitePool>,
    entity_id: &str,
    variables: Option<HashMap<String, String>>,
    user_id: Option<String>,
) -> AppResult<WorkflowResult> {
    let entity = repository::get_entity(&pool, entity_id).await?;

    if entity.type_id != "wf" {
        return Err(AppError::Validation(format!(
            "Entity {} is not a workflow (type: {})",
            entity_id, entity.type_id
        )));
    }

    let definition = resolve_workflow_definition(&entity)?;
    let context =
        create_workflow_context_for_user(entity.id.clone(), pool.clone(), variables, user_id);
    let engine = WorkflowEngine::new(pool)?;

    engine.execute_workflow(&definition, context).await
}

// ---------------------------------------------------------------------------
// Run registry + cooperative cancellation (US-WF-09)
// ---------------------------------------------------------------------------

/// Registry of in-flight runs: `run_id -> cancel token`. Shared between the
/// TUI and the REST API so either can stop a running workflow.
static ACTIVE_RUNS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn active_runs() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    ACTIVE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a run so it can be cancelled later (shares the context token).
fn register_run(run_id: &str, token: Arc<AtomicBool>) {
    if let Ok(mut runs) = active_runs().lock() {
        runs.insert(run_id.to_string(), token);
    }
}

/// Remove a finished run from the registry.
fn unregister_run(run_id: &str) {
    if let Ok(mut runs) = active_runs().lock() {
        runs.remove(run_id);
    }
}

/// Request cooperative cancellation of a running workflow (US-WF-09).
/// Returns `true` when the run was found and the flag was set; the engine
/// stops before the next step and records the run as failed with
/// "cancelled by user".
pub fn cancel_run(run_id: &str) -> bool {
    if let Ok(runs) = active_runs().lock() {
        if let Some(token) = runs.get(run_id) {
            token.store(true, Ordering::Relaxed);
            return true;
        }
    }
    false
}

/// IDs of currently registered (in-flight or pollable) runs.
pub fn active_run_ids() -> Vec<String> {
    active_runs()
        .lock()
        .map(|runs| runs.keys().cloned().collect())
        .unwrap_or_default()
}

/// Spawn a workflow run in the background so the TUI/API stays responsive and
/// the run can be cancelled via [`cancel_run`] (US-WF-09). Returns the
/// assigned `run_id` plus the task handle producing the final result.
pub async fn spawn_workflow_run(
    pool: Arc<SqlitePool>,
    entity_id: &str,
    variables: Option<HashMap<String, String>>,
    user_id: Option<String>,
) -> AppResult<(String, tokio::task::JoinHandle<AppResult<WorkflowResult>>)> {
    let entity = repository::get_entity(&pool, entity_id).await?;
    if entity.type_id != "wf" {
        return Err(AppError::Validation(format!(
            "Entity {} is not a workflow (type: {})",
            entity.id, entity.type_id
        )));
    }
    let definition = resolve_workflow_definition(&entity)?;
    let context =
        create_workflow_context_for_user(entity.id.clone(), pool.clone(), variables, user_id);
    let run_id = context.run_id.clone();
    // Register before spawning so the run is cancellable immediately,
    // even before the task starts executing (execute_workflow re-registers
    // the same token — idempotent).
    register_run(&run_id, context.cancel.clone());
    let engine = WorkflowEngine::new(pool)?;
    let handle = tokio::spawn(async move { engine.execute_workflow(&definition, context).await });
    Ok((run_id, handle))
}

/// Resolve the workflow definition: file-backed workflows (metadata
/// `{"file": "…"}`) load their Lua/JSON definition from disk; everything else
/// comes from the entity content (US-WF-03, scripts in the language of choice).
fn resolve_workflow_definition(entity: &Entity) -> AppResult<WorkflowDefinition> {
    if let Some(path) = metadata_file(entity) {
        let file_content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Validation(format!("cannot read workflow file {path}: {e}")))?;
        if is_json_path(&path) {
            let definition: WorkflowDefinition = serde_json::from_str(&file_content)
                .map_err(|e| AppError::Validation(format!("invalid workflow JSON: {e}")))?;
            return Ok(definition);
        }
        // Lua (or any Lua-based text): single-step workflow
        return Ok(WorkflowDefinition {
            name: entity.name.clone(),
            description: entity.description.clone(),
            steps: vec![WorkflowStep {
                name: "main".to_string(),
                script: file_content,
                depends_on: vec![],
                run_when: None,
            }],
            variables: HashMap::new(),
        });
    }
    WorkflowDefinition::from_entity(entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_lua_script() {
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        let engine = WorkflowEngine::new(pool.clone()).unwrap();

        let script = r#"
            log("Hello from Lua!")
            local result = run_command("echo 'test'")
            log("Command output: " .. result.stdout)
        "#;

        let context = create_workflow_context("test".to_string(), pool, None);
        let result = engine.execute_script(script, context).await.unwrap();

        assert!(result.success);
        assert_eq!(result.steps_completed, 1);
    }

    // ── Workflow definition parsing (visual builder JSON + Lua fallback) ────

    fn entity_with_content(name: &str, content: &str) -> Entity {
        Entity {
            id: "wf-1".to_string(),
            name: name.to_string(),
            description: None,
            content: Some(content.to_string()),
            type_id: "wf".to_string(),
            project_id: None,
            metadata_json: None,
            created_at: String::new(),
            updated_at: String::new(),
            parent_id: None,
        }
    }

    /// Scenario: a visual workflow saved as JSON parses back into steps
    #[test]
    fn given_json_definition_when_parsed_then_steps_preserved() {
        let content = r#"{
            "name": "nightly",
            "description": "backup then clean",
            "steps": [
                {"name": "backup", "script": "print('backing up')", "depends_on": []},
                {"name": "cleanup", "script": "print('cleaning')", "depends_on": []}
            ],
            "variables": {}
        }"#;
        let entity = entity_with_content("nightly", content);

        let definition = WorkflowDefinition::from_entity(&entity).unwrap();
        assert_eq!(definition.steps.len(), 2);
        assert_eq!(definition.steps[0].name, "backup");
        assert_eq!(definition.steps[1].name, "cleanup");
    }

    /// Scenario: a plain Lua workflow falls back to a single "main" step
    #[test]
    fn given_plain_lua_when_parsed_then_single_main_step() {
        let entity = entity_with_content("scripted", "log('hello')");

        let definition = WorkflowDefinition::from_entity(&entity).unwrap();
        assert_eq!(definition.steps.len(), 1);
        assert_eq!(definition.steps[0].name, "main");
        assert!(definition.steps[0].script.contains("hello"));
    }

    /// Scenario: a workflow entity without content is a validation error
    #[test]
    fn given_entity_without_content_when_parsed_then_validation_error() {
        let mut entity = entity_with_content("empty", "x");
        entity.content = None;
        assert!(WorkflowDefinition::from_entity(&entity).is_err());
    }

    /// Scenario: a JSON workflow executes all steps in order
    #[tokio::test]
    async fn given_json_workflow_when_executed_then_all_steps_complete() {
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        let engine = WorkflowEngine::new(pool.clone()).unwrap();

        let definition = WorkflowDefinition {
            name: "two steps".to_string(),
            description: None,
            steps: vec![
                WorkflowStep {
                    name: "first".to_string(),
                    script: "print('one')".to_string(),
                    depends_on: vec![],
                    run_when: None,
                },
                WorkflowStep {
                    name: "second".to_string(),
                    script: "log('two')".to_string(),
                    depends_on: vec![],
                    run_when: None,
                },
            ],
            variables: HashMap::new(),
        };
        let context = create_workflow_context("wf-1".to_string(), pool, None);

        let result = engine.execute_workflow(&definition, context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 2);
        assert!(result.output.contains("first"));
        assert!(result.output.contains("second"));
    }

    /// Scenario: logic nodes — results flow between steps, gates skip steps.
    /// (US-FUT-07)
    #[tokio::test]
    async fn given_logic_workflow_when_executed_then_results_and_gates_apply() {
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        let engine = WorkflowEngine::new(pool.clone()).unwrap();

        let definition = WorkflowDefinition {
            name: "logic".to_string(),
            description: None,
            steps: vec![
                // Produces a boolean result: results["check"] == true
                WorkflowStep {
                    name: "check".to_string(),
                    script: "return 5 > 3".to_string(),
                    depends_on: vec![],
                    run_when: None,
                },
                // AND-style gate over the captured result
                WorkflowStep {
                    name: "gate".to_string(),
                    script: "return results[\"check\"] == true".to_string(),
                    depends_on: vec!["check".to_string()],
                    run_when: None,
                },
                // Runs because gate is true
                WorkflowStep {
                    name: "notify".to_string(),
                    script: "return 'notified'".to_string(),
                    depends_on: vec!["gate".to_string()],
                    run_when: Some("results[\"gate\"] == true".to_string()),
                },
                // Skipped because gate is not false
                WorkflowStep {
                    name: "cleanup".to_string(),
                    script: "return 'cleaned'".to_string(),
                    depends_on: vec!["gate".to_string()],
                    run_when: Some("results[\"gate\"] == false".to_string()),
                },
            ],
            variables: HashMap::new(),
        };
        let context = create_workflow_context("wf-logic".to_string(), pool, None);

        let result = engine.execute_workflow(&definition, context).await.unwrap();
        assert!(result.success);
        // check, gate and notify executed; cleanup skipped
        assert_eq!(result.steps_completed, 3);
        assert!(result.output.contains("notify"));
        assert!(
            result.output.contains("cleanup' skipped"),
            "gated step must be skipped: {}",
            result.output
        );
    }

    /// Scenario: a failing step stops the workflow with an error result
    #[tokio::test]
    async fn given_failing_step_when_executed_then_reports_error() {
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        let engine = WorkflowEngine::new(pool.clone()).unwrap();

        let definition = WorkflowDefinition {
            name: "broken".to_string(),
            description: None,
            steps: vec![WorkflowStep {
                name: "bad".to_string(),
                script: "error('boom')".to_string(),
                depends_on: vec![],
                run_when: None,
            }],
            variables: HashMap::new(),
        };
        let context = create_workflow_context("wf-2".to_string(), pool, None);

        let result = engine.execute_workflow(&definition, context).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_deref().unwrap().contains("boom"));
        assert_eq!(result.steps_completed, 0);
    }

    /// Scenario: executing a non-workflow entity by id is rejected
    #[tokio::test]
    async fn given_command_entity_when_executed_as_workflow_then_validation_error() {
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        crate::db::run_migrations(&pool).await.unwrap();

        let entity = repository::create_entity(
            &pool,
            &crate::models::CreateEntity {
                name: "just a command".to_string(),
                description: None,
                content: Some("ls".to_string()),
                type_id: "cmd".to_string(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();

        let result = execute_workflow_by_id(pool, &entity.id, None).await;
        assert!(result.is_err());
    }

    // ── Type-aware run plans (US-CMD-09 differentiation, file-backed) ───────

    fn entity_typed(type_id: &str, content: &str, metadata: Option<String>) -> Entity {
        Entity {
            id: "e1".to_string(),
            name: "item".to_string(),
            description: None,
            content: Some(content.to_string()),
            type_id: type_id.to_string(),
            project_id: None,
            metadata_json: metadata,
            created_at: String::new(),
            updated_at: String::new(),
            parent_id: None,
        }
    }

    #[test]
    fn cmd_type_runs_via_shell() {
        let plan = build_run_plan(&entity_typed("cmd", "docker ps", None)).unwrap();
        assert_eq!(plan, RunPlan::Shell("docker ps".to_string()));
    }

    #[test]
    fn app_type_is_spawned_detached() {
        let plan = build_run_plan(&entity_typed("app", "firefox", None)).unwrap();
        assert_eq!(plan, RunPlan::App("firefox".to_string()));
    }

    #[test]
    fn script_shebang_uses_interpreter() {
        let plan = build_run_plan(&entity_typed(
            "script",
            "#!/usr/bin/env python3\nprint('x')",
            None,
        ))
        .unwrap();
        match plan {
            RunPlan::Interpreter { program, .. } => assert_eq!(program, "python3"),
            other => panic!("unexpected plan: {:?}", other),
        }
    }

    #[test]
    fn file_metadata_runs_from_file() {
        let metadata = r#"{"file": "tools/backup.py"}"#.to_string();
        let plan = build_run_plan(&entity_typed("script", "unused", Some(metadata))).unwrap();
        match plan {
            RunPlan::Interpreter { program, args } => {
                assert_eq!(program, "python3");
                assert_eq!(args, vec!["tools/backup.py".to_string()]);
            }
            other => panic!("unexpected plan: {:?}", other),
        }
    }

    #[test]
    fn lua_files_run_with_lua() {
        let metadata = r#"{"file": "workflows/nightly.lua"}"#.to_string();
        let plan = build_run_plan(&entity_typed("script", "", Some(metadata))).unwrap();
        match plan {
            RunPlan::Interpreter { program, .. } => assert_eq!(program, "lua"),
            other => panic!("unexpected plan: {:?}", other),
        }
    }

    #[test]
    fn workflows_are_not_shell_run() {
        assert!(build_run_plan(&entity_typed("wf", "log('x')", None)).is_none());
    }

    #[test]
    fn is_json_path_detects_definitions() {
        assert!(is_json_path("wf.json"));
        assert!(!is_json_path("wf.lua"));
    }

    /// Base64 of 32 'a' bytes — a valid 32-byte master key for tests.
    const TEST_KEY: &str = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

    /// Scenario: secrets are exposed to workflow scripts as variables
    #[tokio::test]
    async fn given_user_secret_when_workflow_runs_then_script_reads_variable() {
        std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        crate::db::run_migrations(&pool).await.unwrap();

        let user = repository::get_or_create_user(&pool, "alice")
            .await
            .unwrap();
        let enc = crate::secrets::encrypt_for_user(&pool, &user.id, "topsecret")
            .await
            .unwrap();
        repository::create_secret_full(&pool, &user.id, "api_key", &enc, "api_key", false)
            .await
            .unwrap();

        let script = r#"
            if get_secret("api_key") == "topsecret" then
                log("secret ok")
            else
                error("secret not readable")
            end
        "#;
        let context =
            create_workflow_context_for_user("wf".to_string(), pool.clone(), None, Some(user.id));
        let engine = WorkflowEngine::new(pool).unwrap();
        let result = engine.execute_script(script, context).await.unwrap();
        assert!(result.success, "workflow failed: {:?}", result.error);
    }

    /// Scenario: reauth-protected secrets are not auto-exposed
    #[tokio::test]
    async fn given_reauth_secret_when_workflow_runs_then_not_exposed() {
        std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
        let pool = Arc::new(sqlx::SqlitePool::connect(":memory:").await.unwrap());
        crate::db::run_migrations(&pool).await.unwrap();

        let user = repository::get_or_create_user(&pool, "bob").await.unwrap();
        let enc = crate::secrets::encrypt_for_user(&pool, &user.id, "hidden")
            .await
            .unwrap();
        repository::create_secret_full(&pool, &user.id, "vault", &enc, "ssh_key", true)
            .await
            .unwrap();

        let script = r#"
            if secrets == nil or secrets["vault"] == nil then
                log("protected")
            else
                error("reauth secret leaked")
            end
        "#;
        let context =
            create_workflow_context_for_user("wf".to_string(), pool.clone(), None, Some(user.id));
        let engine = WorkflowEngine::new(pool).unwrap();
        let result = engine.execute_script(script, context).await.unwrap();
        assert!(result.success, "workflow failed: {:?}", result.error);
    }
}

#[cfg(test)]
mod cancel_tests {
    use super::*;

    fn test_pool() -> Arc<SqlitePool> {
        // Not used for the pure-flag tests below.
        Arc::new(SqlitePool::connect_lazy("sqlite::memory:").unwrap())
    }

    #[test]
    fn cancel_run_unknown_id_returns_false() {
        assert!(!cancel_run("no-such-run-id"));
    }

    #[tokio::test]
    async fn cancelled_context_stops_before_first_step() {
        let pool = test_pool();
        let engine = WorkflowEngine::new(pool.clone()).unwrap();
        let context = create_workflow_context("wf".into(), pool, None);
        // Cancel before execution begins
        context.cancel.store(true, Ordering::Relaxed);
        let definition = WorkflowDefinition {
            name: "cancel-me".into(),
            description: None,
            variables: HashMap::new(),
            steps: vec![
                WorkflowStep {
                    name: "never-runs".into(),
                    script: "log('side effect')".into(),
                    depends_on: Vec::new(),
                    run_when: None,
                },
                WorkflowStep {
                    name: "never-runs-2".into(),
                    script: "log('side effect 2')".into(),
                    depends_on: Vec::new(),
                    run_when: None,
                },
            ],
        };
        let result = engine.execute_workflow(&definition, context).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("cancelled"));
        assert_eq!(result.steps_completed, 0);
    }
}
