//! BDD integration scenarios for TUI-OP-HUB.
//!
//! Gherkin-style end-to-end scenarios that exercise the public API the same way
//! the TUI and REST API do: `db` → `repository` → `secrets`/`workflow`.
//! Each test is named `given_<state>_when_<action>_then_<result>` and documented
//! with a `Feature:` / `Scenario:` doc comment.
//!
//! Run with: `cargo test --test bdd_scenarios`

use sqlx::SqlitePool;
use std::sync::Arc;
use tui_op_hub::db;
use tui_op_hub::models::{CreateEntity, CreateProject, WorkflowRun};
use tui_op_hub::repository;
use tui_op_hub::secrets;
use tui_op_hub::tui::list_state::{
    build_workflow_definition, parse_tags_text, VisualStep, VisualWorkflowState,
};
use tui_op_hub::workflow::{self, WorkflowEngine, WorkflowStep};

/// Base64 of 32 'a' bytes — a valid 32-byte master key for tests.
const TEST_KEY: &str = "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=";

/// Given a fresh database: in-memory SQLite (single connection) + migrations.
async fn given_fresh_database() -> Arc<SqlitePool> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db::run_migrations(&pool).await.unwrap();
    Arc::new(pool)
}

/// Given a saved command entity exists.
async fn given_saved_command(
    pool: &SqlitePool,
    name: &str,
    content: &str,
) -> tui_op_hub::models::Entity {
    repository::create_entity(
        pool,
        &CreateEntity {
            name: name.to_string(),
            description: Some(format!("the {} command", name)),
            content: Some(content.to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: Some(parse_tags_text(&format!("{}, e2e", name))),
            metadata_json: None,
        },
    )
    .await
    .unwrap()
}

// ============================================================================
// Feature: Command & knowledge base management (US-CMD-01..09)
// ============================================================================

/// Scenario: full command lifecycle through the repository layer
/// Given an empty knowledge base, when a command is created, edited and deleted,
/// then each step is visible in the stored data.
#[tokio::test]
async fn given_empty_kb_when_command_created_edited_deleted_then_lifecycle_visible() {
    let pool = given_fresh_database().await;

    // When: create
    let cmd = given_saved_command(&pool, "docker-ps", "docker ps -a").await;
    assert_eq!(
        repository::list_entities(&pool, Some("cmd"), None)
            .await
            .unwrap()
            .len(),
        1
    );

    // When: edit
    let edited = repository::update_entity(
        &pool,
        &cmd.id,
        &CreateEntity {
            name: "docker-ps-wide".to_string(),
            description: cmd.description.clone(),
            content: Some("docker ps -a --no-trunc".to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(edited.name, "docker-ps-wide");
    assert_eq!(edited.content.as_deref(), Some("docker ps -a --no-trunc"));

    // When: delete
    repository::delete_entity(&pool, &cmd.id).await.unwrap();
    assert!(repository::list_entities(&pool, Some("cmd"), None)
        .await
        .unwrap()
        .is_empty());
}

/// Scenario: full-text search over the knowledge base
/// Given saved commands, when searching for a keyword, then only matching
/// commands are returned ranked by relevance.
#[tokio::test]
async fn given_saved_commands_when_searched_then_matches_returned() {
    let pool = given_fresh_database().await;
    given_saved_command(&pool, "list-docker", "docker ps -a").await;
    given_saved_command(&pool, "git-status", "git status").await;

    let hits = repository::search_entities(&pool, "docker").await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "list-docker");

    let none = repository::search_entities(&pool, "kubernetes")
        .await
        .unwrap();
    assert!(none.is_empty());
}

// ============================================================================
// Feature: Project organization (US-PROJ-01..07)
// ============================================================================

/// Scenario: group entities under a project
/// Given a project with entities, when listing by project, then only its
/// entities are returned.
#[tokio::test]
async fn given_project_with_entities_when_listed_then_only_project_entities() {
    let pool = given_fresh_database().await;

    let project = repository::create_project(
        &pool,
        &CreateProject {
            name: "homelab".to_string(),
            description: Some("home server stuff".to_string()),
        },
    )
    .await
    .unwrap();

    let mut with_project = CreateEntity {
        name: "backup.sh".to_string(),
        description: None,
        content: Some("rsync -a /data /backup".to_string()),
        type_id: "script".to_string(),
        project_id: Some(project.id.clone()),
        tags: None,
        metadata_json: None,
    };
    repository::create_entity(&pool, &with_project)
        .await
        .unwrap();

    with_project.project_id = None;
    with_project.name = "unrelated".to_string();
    repository::create_entity(&pool, &with_project)
        .await
        .unwrap();

    let in_project = repository::list_entities_by_project(&pool, &project.id)
        .await
        .unwrap();
    assert_eq!(in_project.len(), 1);
    assert_eq!(in_project[0].name, "backup.sh");

    // Renaming the project keeps the association
    let renamed = repository::update_project(
        &pool,
        &project.id,
        &CreateProject {
            name: "homelab-v2".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.name, "homelab-v2");
    assert_eq!(
        repository::list_entities_by_project(&pool, &project.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

// ============================================================================
// Feature: Secrets management (US-SEC-02..04, US-SEC-11..15)
// ============================================================================

/// Scenario: secret round trip stays encrypted at rest
/// Given a user with the master key set, when a secret is encrypted, stored,
/// listed and decrypted, then the plaintext only exists via decryption.
#[tokio::test]
async fn given_user_key_when_secret_stored_then_encrypted_at_rest_and_decryptable() {
    std::env::set_var("TUI_OP_HUB_SECRETS_KEY", TEST_KEY);
    let pool = given_fresh_database().await;

    // Given: the users exist (secrets.user_id references user_profiles.id)
    let alice = repository::get_or_create_user(&pool, "alice")
        .await
        .unwrap()
        .id;
    let bob = repository::get_or_create_user(&pool, "bob")
        .await
        .unwrap()
        .id;

    // When: encrypt + store
    let value_enc = secrets::encrypt_for_user(&pool, &alice, "hunter2")
        .await
        .unwrap();
    let stored = repository::create_secret(&pool, &alice, "github-token", &value_enc)
        .await
        .unwrap();

    // Then: nothing plaintext at rest
    assert_ne!(stored.value_enc, "hunter2");
    assert!(!stored.value_enc.contains("hunter2"));

    // Then: decryptable for the right user
    let decrypted = secrets::decrypt_for_user_id(&pool, &alice, &stored.value_enc)
        .await
        .unwrap();
    assert_eq!(decrypted, "hunter2");

    // Then: listing is per user
    assert_eq!(
        repository::list_secrets(&pool, &alice).await.unwrap().len(),
        1
    );
    assert!(repository::list_secrets(&pool, &bob)
        .await
        .unwrap()
        .is_empty());

    // Then: deleting removes it
    repository::delete_secret(&pool, &stored.id).await.unwrap();
    assert!(repository::list_secrets(&pool, &alice)
        .await
        .unwrap()
        .is_empty());
}

// ============================================================================
// Feature: Visual workflow builder (US-WF-01, US-WF-03, US-WF-06, US-WF-08)
// ============================================================================

/// Scenario: build and execute a workflow from saved commands
/// Given saved commands in the knowledge base, when they are composed into a
/// visual workflow and executed, then every step runs and history is recorded.
#[tokio::test]
async fn given_saved_commands_when_visual_workflow_executed_then_steps_run_and_history_recorded() {
    let pool = given_fresh_database().await;

    // Given: two saved commands
    let backup = given_saved_command(&pool, "step-backup", "print('running backup')").await;
    let cleanup = given_saved_command(&pool, "step-cleanup", "print('running cleanup')").await;

    // When: the visual builder composes them into a workflow
    let steps = vec![
        VisualStep {
            entity_id: backup.id.clone(),
            name: backup.name.clone(),
            script: backup.content.clone().unwrap(),
        },
        VisualStep {
            entity_id: cleanup.id.clone(),
            name: cleanup.name.clone(),
            script: cleanup.content.clone().unwrap(),
        },
    ];
    let definition =
        build_workflow_definition("nightly-job", "backup then cleanup", &steps).unwrap();
    let content = serde_json::to_string_pretty(&definition).unwrap();

    let wf = repository::create_entity(
        &pool,
        &CreateEntity {
            name: "nightly-job".to_string(),
            description: definition.description.clone(),
            content: Some(content),
            type_id: "wf".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    // When: executed like the TUI 'r' key does
    let result = workflow::execute_workflow_by_id(pool.clone(), &wf.id, None)
        .await
        .unwrap();

    // Then: both steps completed in order
    assert!(result.success);
    assert_eq!(result.steps_completed, 2);
    assert!(result.output.contains("step-backup"));
    assert!(result.output.contains("step-cleanup"));

    // Then: run history persisted (US-WF-08)
    repository::insert_workflow_run(
        &pool,
        &WorkflowRun {
            run_id: result.run_id.clone(),
            workflow_id: wf.id.clone(),
            success: result.success,
            output: Some(result.output.clone()),
            error: result.error.clone(),
            duration_ms: Some(result.duration_ms as i64),
            steps_completed: Some(result.steps_completed as i32),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
    .await
    .unwrap();
    let history = repository::list_workflow_runs_by_workflow_id(&pool, &wf.id)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].success);
}

/// Scenario: reordering steps in the builder changes execution order
/// Given a visual workflow with two steps, when the steps are swapped, then the
/// saved definition executes in the new order.
#[tokio::test]
async fn given_visual_workflow_when_steps_swapped_then_execution_order_follows() {
    let pool = given_fresh_database().await;

    let mut builder = VisualWorkflowState::default();
    builder.steps = vec![
        VisualStep {
            entity_id: "a".into(),
            name: "first".into(),
            script: "print('FIRST RAN')".into(),
        },
        VisualStep {
            entity_id: "b".into(),
            name: "second".into(),
            script: "print('SECOND RAN')".into(),
        },
    ];
    builder.selected_step = 1;
    builder.move_step_up(); // swap: second now runs first

    let definition = build_workflow_definition("swapped", "", &builder.steps).unwrap();
    let engine = WorkflowEngine::new(pool.clone()).unwrap();
    let context = workflow::create_workflow_context("wf-swap".to_string(), pool, None);

    let result = engine.execute_workflow(&definition, context).await.unwrap();
    assert!(result.success);
    // Output lists steps in execution order
    let first_pos = result.output.find("second").unwrap();
    let second_pos = result.output.find("first").unwrap();
    assert!(first_pos < second_pos, "swapped step must execute first");
}

/// Scenario: a workflow with a failing step does not report success
/// Given a workflow whose step fails, when executed, then the result is a
/// failure with the step error and zero completed steps.
#[tokio::test]
async fn given_failing_workflow_step_when_executed_then_failure_reported() {
    let pool = given_fresh_database().await;

    let definition = tui_op_hub::workflow::WorkflowDefinition {
        name: "broken".to_string(),
        description: None,
        steps: vec![WorkflowStep {
            name: "explode".to_string(),
            script: "error('intentional failure')".to_string(),
            depends_on: vec![],
        }],
        variables: Default::default(),
    };
    let engine = WorkflowEngine::new(pool.clone()).unwrap();
    let context = workflow::create_workflow_context("wf-broken".to_string(), pool, None);

    let result = engine.execute_workflow(&definition, context).await.unwrap();
    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("intentional failure"));
    assert_eq!(result.steps_completed, 0);
}

// ============================================================================
// Feature: TUI state logic (unit-level BDD)
// ============================================================================

/// Scenario: tags entered in the form are normalized
/// Given a tags text field, when saved, then whitespace is trimmed and empty
/// entries dropped.
#[test]
fn given_tags_text_when_parsed_then_trimmed_and_deduped_of_empties() {
    assert_eq!(parse_tags_text("git , , docker"), vec!["git", "docker"]);
}

/// Scenario: the visual builder rejects saving without a name
/// Given a builder with steps but an empty name, when the definition is built,
/// then an error is returned instead of saving garbage.
#[test]
fn given_builder_without_name_when_saved_then_error() {
    let steps = vec![VisualStep {
        entity_id: "a".into(),
        name: "x".into(),
        script: "print(1)".into(),
    }];
    assert!(build_workflow_definition("   ", "", &steps).is_err());
    assert!(build_workflow_definition("ok", "", &[]).is_err());
}

// ============================================================================
// Feature: Settings, theming & keybindings (US-APP-01..06)
// ============================================================================

/// Scenario: a customized config survives a save/load round trip
/// Given a config with a custom editor, page size, theme and rebinding, when it
/// is saved and reloaded, then every setting is preserved.
#[test]
fn given_customized_config_when_saved_then_loads_identically() {
    let mut cfg = tui_op_hub::config::AppConfig::default();
    cfg.general.editor = "nvim --wait".to_string();
    cfg.tui.page_size = 30;
    cfg.theme.name = "dracula".to_string();
    cfg.keybindings.set("copy", "Y".to_string());

    let path =
        std::env::temp_dir().join(format!("tui-op-hub-bdd-cfg-{}.toml", uuid::Uuid::new_v4()));
    cfg.save(&path).unwrap();
    let loaded = tui_op_hub::config::AppConfig::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.general.editor, "nvim --wait");
    assert_eq!(loaded.tui.page_size, 30);
    assert_eq!(loaded.theme.name, "dracula");
    assert_eq!(loaded.keybindings.get("copy"), "Y");
}

/// Scenario: rebound keybindings resolve for their actions
/// Given custom bindings, when the action key codes are looked up, then every
/// action resolves to its bound key (invalid text falls back safely).
#[test]
fn given_custom_keybindings_when_resolved_then_actions_use_them() {
    use tui_op_hub::config::KeybindingsConfig;

    let mut kb = tui_op_hub::config::KeybindingsConfig::default();
    for action in KeybindingsConfig::ACTIONS {
        // Every default must parse into a real key
        assert!(KeybindingsConfig::to_keycode(kb.get(action)).is_some());
    }

    kb.set("create", "N".to_string());
    assert_eq!(kb.key_for("create"), crossterm::event::KeyCode::Char('N'));

    // Serialization round trip for rebind capture
    let pressed =
        KeybindingsConfig::keycode_to_string(crossterm::event::KeyCode::Char('Z')).unwrap();
    assert_eq!(
        KeybindingsConfig::to_keycode(&pressed),
        Some(crossterm::event::KeyCode::Char('Z'))
    );
}

/// Scenario: the theme config maps to the matching palette
/// Given the nord preset, when the theme is built from config, then its colors
/// differ from the default; an unknown name builds a custom theme from the
/// configured fg/bg/accent values.
#[test]
fn given_theme_preset_when_mapped_then_colors_change() {
    // Unknown preset name = custom theme: fg/bg/accent from the config apply
    let unknown = tui_op_hub::config::ThemeConfig {
        name: "does-not-exist".to_string(),
        ..Default::default()
    };
    let custom = tui_op_hub::tui::modern_ui::ModernTheme::from_config(&unknown);
    assert_eq!(custom.bg, unknown.bg_color());
    assert_eq!(custom.fg, unknown.fg_color());

    // A real preset maps to its distinct palette (fg/bg are NOT re-applied)
    let nord = tui_op_hub::config::ThemeConfig {
        name: "nord".to_string(),
        ..Default::default()
    };
    let nord_theme = tui_op_hub::tui::modern_ui::ModernTheme::from_config(&nord);
    assert_ne!(
        nord_theme.bg,
        tui_op_hub::tui::modern_ui::ModernTheme::default().bg
    );
}

/// Scenario: a custom theme is defined entirely in the config file
/// Given a theme config with a custom name and hex colors, when the theme is
/// built, then the defined colors are used and unset ones fall back.
#[test]
fn given_custom_theme_colors_when_mapped_then_overrides_apply() {
    let cfg = tui_op_hub::config::ThemeConfig {
        name: "myscheme".to_string(),
        bg: "#101010".to_string(),
        fg: "#e0e0e0".to_string(),
        accent: "#ff8800".to_string(),
        primary: Some("#3366ff".to_string()),
        error: Some("#cc0000".to_string()),
        ..Default::default()
    };
    let theme = tui_op_hub::tui::modern_ui::ModernTheme::from_config(&cfg);

    assert_eq!(theme.bg, ratatui::style::Color::Rgb(0x10, 0x10, 0x10));
    assert_eq!(theme.primary, ratatui::style::Color::Rgb(0x33, 0x66, 0xff));
    assert_eq!(theme.error, ratatui::style::Color::Rgb(0xcc, 0x00, 0x00));
    // Unset optional color falls back to the default palette
    assert_eq!(
        theme.warning,
        tui_op_hub::tui::modern_ui::ModernTheme::default().warning
    );
}

/// Scenario: advanced visual config persists to the config file
/// Given the advanced screen with a cycled color and edited option, when the
/// config is saved, then the palette and system options load back from disk.
#[test]
fn given_advanced_visual_edits_when_saved_then_persisted() {
    let mut cfg = tui_op_hub::config::AppConfig::default();

    // Simulate the visual color cycling on the bg row and a hex override
    let palette_next = "#ff6600";
    cfg.theme.bg = palette_next.to_string();
    cfg.theme.primary = Some("#58a6ff".to_string());
    cfg.database.busy_timeout_ms = 2500;
    cfg.database.path = "custom.db".to_string();

    let path =
        std::env::temp_dir().join(format!("tui-op-hub-bdd-adv-{}.conf", uuid::Uuid::new_v4()));
    cfg.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let loaded = tui_op_hub::config::AppConfig::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    // Hyprland-style file carries the visual edits
    assert!(text.contains("bg = \"#ff6600\""));
    assert!(text.contains("primary = \"#58a6ff\""));
    assert_eq!(loaded.theme.bg, "#ff6600");
    assert_eq!(loaded.theme.primary.as_deref(), Some("#58a6ff"));
    assert_eq!(loaded.database.busy_timeout_ms, 2500);
    assert_eq!(loaded.database.path, "custom.db");

    // And the theme engine maps them onto the live palette
    let theme = tui_op_hub::tui::modern_ui::ModernTheme::from_config(&loaded.theme);
    assert_eq!(theme.primary, ratatui::style::Color::Rgb(0x58, 0xa6, 0xff));
}

// ============================================================================
// Feature: Phase 2 — admin users, file-backed workflows (US-SEC, US-WF)
// ============================================================================

/// Scenario: the first user is admin and admins manage users
/// Given a fresh database, when the first user signs up, then they are admin;
/// a second user is not, and deleting a user removes their secrets (the
/// forgotten-password escape hatch).
#[tokio::test]
async fn given_fresh_db_when_users_signup_then_first_is_admin_and_can_delete_users() {
    let pool = given_fresh_database().await;

    // First user becomes admin
    let admin = tui_op_hub::auth::AuthManager::new(pool.clone());
    let admin_id = admin.create_user("admin", "adminpass1").await.unwrap();
    assert!(
        repository::is_admin(&pool, &admin_id).await.unwrap(),
        "first user must be admin"
    );

    // Second user is not admin
    let second = tui_op_hub::auth::AuthManager::new(pool.clone());
    let user_id = second.create_user("regular", "regularpass1").await.unwrap();
    assert!(!repository::is_admin(&pool, &user_id).await.unwrap());

    // Regular user cannot reset passwords
    let denied = tui_op_hub::auth::admin_reset_password(&pool, &user_id, "admin", "newpass1").await;
    assert!(denied.is_err(), "non-admin reset must be denied");

    // Admin can delete a user; their secrets are removed with them (FK cascade)
    repository::get_or_create_user(&pool, "victim")
        .await
        .unwrap();
    let enc = secrets::encrypt_for_user(&pool, &user_id, "victim-secret")
        .await
        .unwrap();
    repository::create_secret(&pool, &user_id, "secret", &enc)
        .await
        .unwrap();
    assert_eq!(
        repository::list_secrets(&pool, &user_id)
            .await
            .unwrap()
            .len(),
        1
    );

    repository::delete_user(&pool, &user_id).await.unwrap();
    assert!(repository::list_secrets(&pool, &user_id)
        .await
        .unwrap()
        .is_empty());
    assert!(
        repository::delete_user(&pool, &user_id).await.is_err(),
        "already gone"
    );
}

/// Scenario: file-backed workflows and scripts run from disk
/// Given a JSON workflow definition file referenced via metadata, when the
/// workflow entity is executed, then the steps are read from the file.
#[tokio::test]
async fn given_file_backed_workflow_when_executed_then_definition_loaded_from_file() {
    let pool = given_fresh_database().await;
    let dir = std::env::temp_dir().join(format!("tui-op-hub-bdd-wf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let wf_path = dir.join("nightly.json");
    std::fs::write(
        &wf_path,
        r#"{
            "name": "nightly",
            "steps": [
                {"name": "step-one", "script": "print('one ok')", "depends_on": []},
                {"name": "step-two", "script": "log('done')", "depends_on": []}
            ],
            "variables": {}
        }"#,
    )
    .unwrap();

    let wf = repository::create_entity(
        &pool,
        &CreateEntity {
            name: "nightly".to_string(),
            description: None,
            content: None,
            type_id: "wf".to_string(),
            project_id: None,
            tags: None,
            metadata_json: Some(
                serde_json::json!({ "file": wf_path.display().to_string() }).to_string(),
            ),
        },
    )
    .await
    .unwrap();

    let result = workflow::execute_workflow_by_id(pool.clone(), &wf.id, None)
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.steps_completed, 2);

    let _ = std::fs::remove_dir_all(&dir);
}
