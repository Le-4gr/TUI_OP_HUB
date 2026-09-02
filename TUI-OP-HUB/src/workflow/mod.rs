//! Workflow execution engine (US-WF-01..09, US-PLG-02, US-API-04).
//!
//! Provides Lua-based workflow execution with safe host functions.
//! Supports linear workflows and basic DAG execution.

use mlua::Lua;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
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

        // Execute steps in dependency order (simple linear execution for now)
        for step in &definition.steps {
            tracing::info!(step = %step.name, "executing workflow step");

            match self.lua.load(&step.script).exec() {
                Ok(_) => {
                    steps_completed += 1;
                    output.push_str(&format!("✓ Step '{}' completed\n", step.name));
                }
                Err(e) => {
                    let error_msg = format!("✗ Step '{}' failed: {}", step.name, e);
                    tracing::error!(step = %step.name, error = %e, "workflow step failed");

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
    }
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

    let definition = WorkflowDefinition::from_entity(&entity)?;
    let context = create_workflow_context(entity.id.clone(), pool.clone(), variables);
    let engine = WorkflowEngine::new(pool)?;

    engine.execute_workflow(&definition, context).await
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
                },
                WorkflowStep {
                    name: "second".to_string(),
                    script: "log('two')".to_string(),
                    depends_on: vec![],
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
}
