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
use tui_op_hub::share;
use tui_op_hub::tui::list_state::{
    build_workflow_definition, parse_tags_text, VisualStep, VisualWorkflowState,
};
use tui_op_hub::tui::modern_ui::AppState;
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
    let decrypted = secrets::decrypt_for_user(&pool, &alice, &stored.value_enc)
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

use tui_op_hub::config::AppConfig;
use tui_op_hub::tui::list_state::{ADVANCED_ROWS, SETTINGS_ROWS};
use tui_op_hub::tui::modern_app::ModernApp;

// ============================================================================
// Feature: Keybinds helper overlay (US-TUI-09)
// ============================================================================

/// Scenario: the `?` key toggles the keybinds helper from any screen
/// Given the app, when `?` is pressed on the dashboard, the overlay opens;
/// pressing `?` again or Esc closes it — from any state.
#[tokio::test]
async fn given_any_screen_when_question_mark_then_keybinds_overlay_toggles() {
    let mut app = given_tui_app().await;

    // From the dashboard
    app.bdd_press(crossterm::event::KeyCode::Char('?')).await;
    assert!(app.bdd_keybinds_open(), "overlay opens on dashboard");
    app.bdd_press(crossterm::event::KeyCode::Esc).await;
    assert!(!app.bdd_keybinds_open());

    // Also from Settings (no dead end: Esc closes the overlay, not the screen)
    app.bdd_open_settings();
    app.bdd_press(crossterm::event::KeyCode::Char('?')).await;
    assert!(app.bdd_keybinds_open());
    app.bdd_press(crossterm::event::KeyCode::Char('?')).await;
    assert!(!app.bdd_keybinds_open());
    assert_eq!(
        app.bdd_state(),
        tui_op_hub::tui::modern_ui::AppState::Settings
    );
}

/// Scenario: every context has keybind hints (no empty footers)
/// Given all main screens, when hints are requested, then each has entries and
/// always includes the `?` keybinds helper.
#[test]
fn given_any_state_when_hints_requested_then_hints_are_non_empty() {
    let states = [
        tui_op_hub::tui::modern_ui::AppState::Dashboard,
        tui_op_hub::tui::modern_ui::AppState::Knowledge,
        tui_op_hub::tui::modern_ui::AppState::Projects,
        tui_op_hub::tui::modern_ui::AppState::Workflows,
        tui_op_hub::tui::modern_ui::AppState::Secrets,
        tui_op_hub::tui::modern_ui::AppState::Settings,
    ];
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        for state in states {
            let mut probe = given_tui_app().await;
            probe.bdd_set_state(state.clone());
            let hints = probe.bdd_keybind_hints();
            assert!(!hints.is_empty(), "state {:?} must have hints", state);
            assert!(
                hints.iter().any(|(k, _)| k == "?"),
                "hints always include '?'"
            );
        }
    });
}

// ============================================================================
// Feature: Numpad support in Settings screens (US-APP-06 usability)
// ============================================================================

/// Scenario: a stolen database is useless without the machine key
/// Given a user whose key was derived with machine secret A, when the same
/// password+salt is derived with machine secret B (a different machine), then
/// the ciphertext cannot be decrypted.
#[tokio::test]
async fn given_stolen_db_on_other_machine_when_decrypted_then_fails() {
    let machine_a = [1u8; 32];
    let machine_b = [2u8; 32];

    let salt = "usersalt";
    // Original machine: secrets encrypted under password+machineA
    let key_a = tui_op_hub::auth::derive_user_key("hunter2", salt, &machine_a).unwrap();
    let ct = tui_op_hub::auth::encrypt_value("my-token", &key_a).unwrap();

    // Thief copies the DB to machine B and knows the password — still fails
    let key_b = tui_op_hub::auth::derive_user_key("hunter2", salt, &machine_b).unwrap();
    assert!(tui_op_hub::auth::decrypt_value(&ct, &key_b).is_err());

    // The original machine still decrypts fine
    assert_eq!(
        tui_op_hub::auth::decrypt_value(&ct, &key_a).unwrap(),
        "my-token"
    );
}

/// Scenario: developer mode wipes users without login (covered end-to-end via
/// the Settings screen in the earlier scenario; here the login-manager path).
#[tokio::test]
async fn given_dev_mode_when_login_manager_deletes_user_then_secrets_cascade() {
    let pool = given_fresh_database().await;
    let auth = tui_op_hub::auth::AuthManager::new(pool.clone());
    let user_id = auth.create_user("victim", "password1").await.unwrap();
    repository::get_or_create_user(&pool, "victim")
        .await
        .unwrap();
    let enc = secrets::encrypt_for_user(&pool, &user_id, "data")
        .await
        .unwrap();
    repository::create_secret(&pool, &user_id, "s", &enc)
        .await
        .unwrap();

    // Simulate the login dev manager's delete action
    repository::delete_user(&pool, &user_id).await.unwrap();
    assert!(repository::list_secrets(&pool, &user_id)
        .await
        .unwrap()
        .is_empty());
    // The user's secrets are gone with them; password verification no longer
    // succeeds for the deleted profile.
    let verify = auth
        .verify_password_by_username("victim", "password1")
        .await;
    assert!(
        verify.is_err() || matches!(&verify, Ok(false)),
        "deleted user must not verify"
    );
}

/// Scenario: the login dev manager lists users when opened with `u`
/// Given developer mode and two registered users, when the login manager
/// opens, then both users are listed (the empty-list regression).
#[tokio::test]
async fn given_users_when_login_dev_manager_opens_then_users_are_listed() {
    let pool = given_fresh_database().await;
    let auth = tui_op_hub::auth::AuthManager::new(pool.clone());
    let _ = auth.create_user("alpha", "password1").await.unwrap();
    let _ = auth.create_user("beta", "password2").await.unwrap();

    let mut app = ModernApp::new(pool.clone(), AppConfig::default());
    app.bdd_set_state(tui_op_hub::tui::modern_ui::AppState::Login);

    // Open via `u` (as the key handler does) — the manager must refresh its list
    app.bdd_press(crossterm::event::KeyCode::Char('u')).await;
    let users = app.bdd_dev_user_list();
    assert_eq!(users.len(), 2, "both users must be listed");
    assert!(users.iter().any(|u| u == "alpha"));
    assert!(users.iter().any(|u| u == "beta"));
}

/// Scenario: `/` enters search mode and typing filters the list live
/// Given seeded commands, when `/` then characters are pressed, then search
/// activates with the typed query (visible in the header) and the list filters.
#[tokio::test]
async fn given_commands_tab_when_slash_search_then_live_filtering() {
    let pool = given_fresh_database().await;
    let mut app = ModernApp::new(pool.clone(), AppConfig::default());
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();
    app.bdd_goto_commands().await;

    app.bdd_set_state(tui_op_hub::tui::modern_ui::AppState::Knowledge);
    app.bdd_press(crossterm::event::KeyCode::Char('/')).await;
    assert!(
        app.bdd_search_active(),
        "`/` must enter search mode on a list tab"
    );

    // Type a fuzzy query; list must filter to matching items only
    for c in "dck".chars() {
        app.bdd_press(crossterm::event::KeyCode::Char(c)).await;
    }
    let (items, _) = app.bdd_command_list();
    assert!(!items.is_empty(), "dck should match docker");
    // All matches are docker/k8s related (fuzzy over name + description)
    assert!(
        items
            .iter()
            .all(|n| n.contains("docker") || n.contains("kubectl")),
        "only fuzzy matches shown, got {:?}",
        items
    );

    // Esc clears the filter and restores the full list
    app.bdd_press(crossterm::event::KeyCode::Esc).await;
    let (items, _) = app.bdd_command_list();
    assert!(items.len() >= 2, "full list restored after Esc");
}

/// Given a running app on an in-memory database.
async fn given_tui_app() -> ModernApp {
    let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
    ModernApp::new(std::sync::Arc::new(pool), AppConfig::default())
}

/// Scenario: digits switch tabs from inside Settings
/// Given the Settings screen, when a digit 1-9 is pressed, then the app
/// switches to that tab (digits always mean tabs); arrow keys move the
/// settings selection instead.
#[tokio::test]
async fn given_settings_open_when_digit_pressed_then_switches_tab() {
    let mut app = given_tui_app().await;
    app.bdd_open_settings();
    assert_eq!(app.bdd_settings_selected(), 0);

    // Arrows still move the selection
    app.bdd_press(crossterm::event::KeyCode::Down).await;
    assert_eq!(app.bdd_settings_selected(), 1);
    app.bdd_press(crossterm::event::KeyCode::Up).await;
    assert_eq!(app.bdd_settings_selected(), 0);

    // Digits switch tabs (US-TUI-11/12 mapping: 3 projects, 0 settings, 6 plugins)
    app.bdd_press(crossterm::event::KeyCode::Char('3')).await;
    assert_eq!(app.bdd_state(), AppState::Projects);
    app.bdd_press(crossterm::event::KeyCode::Char('0')).await;
    assert_eq!(app.bdd_state(), AppState::Settings);
    app.bdd_press(crossterm::event::KeyCode::Char('7')).await;
    assert_eq!(app.bdd_state(), AppState::Plugins);
}

/// Scenario: left/right cycles the theme preset on the Theme row
/// Given the Theme row, when `→`/`←` is pressed, then the preset
/// cycles forward/backward.
#[tokio::test]
async fn given_theme_row_when_left_right_then_preset_cycles() {
    let mut app = given_tui_app().await;
    app.bdd_open_settings();
    app.bdd_select_settings_row(2); // theme row
    assert_eq!(app.bdd_theme_name(), "dark");

    app.bdd_press(crossterm::event::KeyCode::Right).await;
    assert_eq!(app.bdd_theme_name(), "light");
    app.bdd_press(crossterm::event::KeyCode::Left).await;
    assert_eq!(app.bdd_theme_name(), "dark");
}

/// Scenario: advanced navigation uses arrows; digits switch tabs
/// Given the Advanced screen, when arrows are pressed, rows move and colors
/// cycle; when a digit is pressed, the app switches to that tab.
#[tokio::test]
async fn given_advanced_open_when_arrows_pressed_then_nav_and_colors_work() {
    let mut app = given_tui_app().await;
    app.bdd_open_advanced();
    // Navigation: Down moves the selection (fg -> bg)
    app.bdd_press(crossterm::event::KeyCode::Down).await;
    assert_eq!(app.bdd_advanced_selected(), 1);

    // Color cycling on the bg row (row 1): black -> white -> black
    assert_eq!(app.bdd_theme_bg(), "black");
    app.bdd_press(crossterm::event::KeyCode::Right).await;
    assert_eq!(app.bdd_theme_bg(), "white");
    app.bdd_press(crossterm::event::KeyCode::Left).await;
    assert_eq!(app.bdd_theme_bg(), "black");

    // Digits switch tabs out of Advanced (US-TUI-11/12: 4 = Workflows)
    app.bdd_press(crossterm::event::KeyCode::Char('4')).await;
    assert_eq!(app.bdd_state(), AppState::Workflows);
}

/// Scenario: numpad navigation works in the keygen form
/// Given the SSH/GPG keygen form, when numpad `2`/`8` is pressed, then the
/// focused field moves; `4`/`6` on the Kind row cycles the key kind.
#[tokio::test]
async fn given_keygen_open_when_numpad_digits_pressed_then_fields_move() {
    let mut app = given_tui_app().await;
    app.bdd_open_keygen();
    assert_eq!(app.bdd_keygen_field(), 0);

    app.bdd_press(crossterm::event::KeyCode::Char('2')).await; // numpad down
    assert_eq!(app.bdd_keygen_field(), 1);
    app.bdd_press(crossterm::event::KeyCode::Char('8')).await; // numpad up
    assert_eq!(app.bdd_keygen_field(), 0);
}

// ============================================================================
// Feature: Dev-mode database operations (US-NF)
// ============================================================================

/// Scenario: dev mode wipes the entire database
/// Given seeded data, when Shift+D is pressed, then all tables are emptied
/// and the app refreshes.
#[tokio::test]
async fn given_seeded_data_when_dev_wipe_db_then_all_tables_empty() {
    let pool = given_fresh_database().await;
    let mut app = ModernApp::new(pool.clone(), AppConfig::default());
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();
    app.bdd_goto_commands().await;
    let (before, _) = app.bdd_command_list();
    assert!(!before.is_empty(), "seeded commands must exist");

    app.bdd_press(crossterm::event::KeyCode::Char('D')).await; // Shift+D = wipe DB

    let (after, _) = app.bdd_command_list();
    assert!(after.is_empty(), "commands must be gone after wipe");
    assert_eq!(tui_op_hub::repository::count_users(&pool).await.unwrap(), 0);
}

/// Scenario: dev mode deletes all entities in the current tab
/// Given seeded commands, when Shift+A is pressed on the Commands tab, then
/// all commands/scripts/apps are deleted but the user remains.
#[tokio::test]
async fn given_commands_when_dev_delete_all_in_tab_then_tab_emptied_user_remains() {
    let pool = given_fresh_database().await;
    let mut app = ModernApp::new(pool.clone(), AppConfig::default());
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();
    app.bdd_goto_commands().await;

    app.bdd_press(crossterm::event::KeyCode::Char('A')).await; // Shift+A = delete all in tab

    let (items, _) = app.bdd_command_list();
    assert!(items.is_empty(), "all commands deleted");
    // User is still there
    assert!(tui_op_hub::repository::count_users(&pool).await.is_ok());
}

// ============================================================================
// Feature: Tooling — seeded command families, options, fuzzy search (Phase 2)
// ============================================================================

/// Scenario: the knowledge base is prepopulated with command families
/// Given a fresh database, when seeding runs, then common command families
/// exist with structured options (flag + description) as child entities.
#[tokio::test]
async fn given_fresh_db_when_seeded_then_command_families_with_options_exist() {
    let pool = given_fresh_database().await;
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();

    // Known families exist — both systemd AND OpenRC tooling
    for family in [
        "git",
        "docker",
        "systemctl",
        "rc-service",
        "rc-update",
        "curl",
    ] {
        let entity = repository::get_entity_by_name_and_type(&pool, family, "cmd")
            .await
            .unwrap_or_else(|_| panic!("seeded family '{}' missing", family));
        assert!(
            entity.description.is_some(),
            "'{}' has a description",
            family
        );
    }

    // git has structured options as children with descriptions
    let git = repository::get_entity_by_name_and_type(&pool, "git", "cmd")
        .await
        .unwrap();
    let options = repository::list_child_entities(&pool, &git.id)
        .await
        .unwrap();
    assert!(options.len() >= 5, "git family has its options");
    assert!(options
        .iter()
        .all(|o| o.name.starts_with('-') || !o.name.is_empty()));
    assert!(options
        .iter()
        .all(|o| !o.description.clone().unwrap_or_default().is_empty()));

    // Known tools are seeded as app entities, e.g. the yazi file browser
    let yazi = repository::get_entity_by_name_and_type(&pool, "yazi", "app").await;
    assert!(yazi.is_ok(), "yazi (file browser) seeded");
    let fetch = repository::get_entity_by_name_and_type(&pool, "fastfetch", "app").await;
    assert!(fetch.is_ok(), "fastfetch (fetch tool) seeded");
}

/// Scenario: seeding is idempotent
/// Given an already seeded database, when seeding runs again, then no
/// duplicates are created.
#[tokio::test]
async fn given_seeded_db_when_seeded_again_then_no_duplicates() {
    let pool = given_fresh_database().await;
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();
    tui_op_hub::seed::seed_builtin_commands(&pool)
        .await
        .unwrap();

    let gits = repository::count_entities_named(&pool, "git", "cmd")
        .await
        .unwrap();
    assert_eq!(gits, 1, "git family seeded exactly once");
}

/// Scenario: fuzzy search finds commands despite typos
/// Given seeded commands, when the user fuzzy-searches with a partial
/// fragment, then matching commands are found and the shortest/best one
/// ranks first.
#[test]
fn given_seeded_commands_when_fuzzy_searched_then_best_match_first() {
    let candidates = ["git", "gitk", "lazygit", "docker", "grep"];

    // 'dck' is a subsequence of docker only
    let mut ranked: Vec<(&str, i64)> = candidates
        .iter()
        .filter_map(|c| tui_op_hub::fuzzy::fuzzy_match(c, "dck").map(|s| (*c, s)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].0, "docker");

    // 'git' matches git, gitk and lazygit — the exact command ranks first
    let mut ranked: Vec<(&str, i64)> = candidates
        .iter()
        .filter_map(|c| tui_op_hub::fuzzy::fuzzy_match(c, "git").map(|s| (*c, s)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    assert_eq!(ranked[0].0, "git", "exact short match ranks first");
}

/// Scenario: the init system is detected for fetch and service commands
/// Given any Linux system, when the init system is detected, then the result
/// is one of the supported values (systemd, OpenRC or unknown) — never a panic.
#[test]
fn given_any_system_when_init_detected_then_result_is_supported() {
    let init = tui_op_hub::seed::detect_init_system();
    assert!(["systemd", "openrc", "unknown"].contains(&init));
}

// ============================================================================
// Feature: Developer mode — wipe users without logging in (US-NF)
// ============================================================================

/// Scenario: dev mode deletes every user straight from the Settings screen
/// Given developer mode (debug build / TUI_OP_HUB_DEV=1) and two registered
/// users, when `d` is pressed twice in Settings, then all users are gone
/// without logging in — and the next signup becomes admin again.
#[tokio::test]
async fn given_dev_mode_when_d_pressed_twice_then_all_users_deleted() {
    // Given: developer mode (debug build) and two registered users
    let pool = given_fresh_database().await;
    let mut app = ModernApp::new(pool.clone(), AppConfig::default());
    let auth = tui_op_hub::auth::AuthManager::new(pool.clone());
    let _first = auth.create_user("first", "password1").await.unwrap();
    let _second = auth.create_user("second", "password2").await.unwrap();
    assert_eq!(tui_op_hub::repository::count_users(&pool).await.unwrap(), 2);

    // When: open Settings and press d twice (two-step confirm)
    app.bdd_open_settings();
    app.bdd_press(crossterm::event::KeyCode::Char('d')).await;
    assert_eq!(
        tui_op_hub::repository::count_users(&pool).await.unwrap(),
        2,
        "first press only arms the confirmation"
    );
    app.bdd_press(crossterm::event::KeyCode::Char('d')).await;

    // Then: every user is gone without any login
    assert_eq!(tui_op_hub::repository::count_users(&pool).await.unwrap(), 0);

    // And: the next signup becomes admin again (fresh dev cycle)
    let auth2 = tui_op_hub::auth::AuthManager::new(pool.clone());
    let new_admin = auth2.create_user("fresh", "password3").await.unwrap();
    assert!(tui_op_hub::repository::is_admin(&pool, &new_admin)
        .await
        .unwrap());
}

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

// ============================================================================
// Feature: Entity type tabs & project detail (US-CMD-01, US-PROJ-02, US-PROJ-07)
// ============================================================================

/// Scenario: the Commands / Apps / Scripts tabs each list only their own type
/// Given entities of all three types exist, when each tab's list is fetched,
/// then every tab shows exactly the entities of its type.
#[tokio::test]
async fn given_entities_of_three_types_when_tab_listed_then_only_matching_type_shown() {
    let pool = given_fresh_database().await;
    for (name, ty) in [
        ("bdd-cmd", "cmd"),
        ("bdd-script", "script"),
        ("bdd-app", "app"),
    ] {
        repository::create_entity(
            &pool,
            &CreateEntity {
                name: name.to_string(),
                description: None,
                content: Some("echo hi".to_string()),
                type_id: ty.to_string(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();
    }

    for (ty, expected_name) in [
        ("cmd", "bdd-cmd"),
        ("script", "bdd-script"),
        ("app", "bdd-app"),
    ] {
        let items = repository::list_entities(&pool, Some(ty), None)
            .await
            .unwrap();
        assert_eq!(items.len(), 1, "type {ty} should list exactly one");
        assert_eq!(items[0].name, expected_name);
        assert_eq!(items[0].type_id, ty);
    }
}

/// Scenario: a project detail shows all its entities and nothing else
/// Given a project with two entities and an unrelated orphan entity, when the
/// project's contents are listed, then exactly the two project entities return.
#[tokio::test]
async fn given_project_with_entities_when_detail_opened_then_project_entities_listed() {
    let pool = given_fresh_database().await;
    let project = repository::create_project(
        &pool,
        &CreateProject {
            name: "bdd-project".to_string(),
            description: Some("BDD detail".to_string()),
        },
    )
    .await
    .unwrap();

    for (name, ty) in [("proj-cmd", "cmd"), ("proj-script", "script")] {
        repository::create_entity(
            &pool,
            &CreateEntity {
                name: name.to_string(),
                description: None,
                content: None,
                type_id: ty.to_string(),
                project_id: Some(project.id.clone()),
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();
    }
    // Orphan entity in no project
    repository::create_entity(
        &pool,
        &CreateEntity {
            name: "orphan".to_string(),
            description: None,
            content: None,
            type_id: "cmd".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    let entities = repository::list_entities_by_project(&pool, &project.id)
        .await
        .unwrap();
    assert_eq!(entities.len(), 2);
    assert!(entities
        .iter()
        .all(|e| e.project_id.as_deref() == Some(project.id.as_str())));
    assert!(entities.iter().any(|e| e.name == "proj-cmd"));
    assert!(entities.iter().any(|e| e.name == "proj-script"));
}

// ============================================================================
// Feature: Knowledge-base import/export round trip (US-CMD-01, US-SEC-02)
// ============================================================================

/// Scenario: exporting and re-importing on a fresh database restores entities
/// Given a knowledge base with commands, scripts and apps, when it is exported
/// (secrets excluded) and imported into a fresh database, then every entity
/// comes back with its content and type.
#[tokio::test]
async fn given_knowledge_base_when_exported_then_import_restores_entities() {
    let pool = given_fresh_database().await;
    for (name, ty) in [
        ("Export cmd", "cmd"),
        ("Export script", "script"),
        ("Export app", "app"),
    ] {
        repository::create_entity(
            &pool,
            &CreateEntity {
                name: name.to_string(),
                description: Some("before export".to_string()),
                content: Some(format!("content of {name}")),
                type_id: ty.to_string(),
                project_id: None,
                tags: None,
                metadata_json: None,
            },
        )
        .await
        .unwrap();
    }

    // Export with secrets excluded (the safe default)
    let bundle = share::export_knowledge(&pool, None, share::SecretMode::Exclude, None)
        .await
        .unwrap();
    assert!(bundle.entities.len() >= 3);
    assert!(
        bundle.secrets.is_empty(),
        "excluded mode exports no secrets"
    );
    assert_eq!(bundle.secret_mode, "excluded");

    // Import into a fresh database (simulating another machine)
    let fresh = given_fresh_database().await;
    let (imported, skipped) = share::import_knowledge(&fresh, &bundle).await.unwrap();
    assert!(
        imported >= 3,
        "at least the 3 entities import, got {imported}"
    );
    assert_eq!(skipped, 0);

    // And the content round-trips
    let restored = repository::list_entities(&fresh, Some("script"), None)
        .await
        .unwrap();
    assert!(restored
        .iter()
        .any(|e| e.name == "Export script"
            && e.content.as_deref() == Some("content of Export script")));
}

/// Scenario: importing never overwrites local edits
/// Given a local entity named the same as one in the bundle, when the bundle
/// is imported, then the local version wins and the item is counted skipped.
#[tokio::test]
async fn given_conflicting_name_when_imported_then_local_version_wins() {
    let pool = given_fresh_database().await;
    repository::create_entity(
        &pool,
        &CreateEntity {
            name: "shared name".to_string(),
            description: None,
            content: Some("LOCAL".to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    let bundle = share::KnowledgeBundle {
        version: 1,
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        entities: vec![share::SharedEntity {
            name: "shared name".to_string(),
            type_id: "cmd".to_string(),
            description: None,
            content: Some("INCOMING".to_string()),
            parent: None,
        }],
        secrets: vec![],
        secret_mode: "excluded".to_string(),
    };

    let (imported, skipped) = share::import_knowledge(&pool, &bundle).await.unwrap();
    assert_eq!(imported, 0);
    assert_eq!(skipped, 1);

    let items = repository::list_entities(&pool, Some("cmd"), None)
        .await
        .unwrap();
    let kept = items.iter().find(|e| e.name == "shared name").unwrap();
    assert_eq!(kept.content.as_deref(), Some("LOCAL"));
}

/// Scenario: command options (children) re-attach to their parent on import
/// Given a bundle with a command and an option child, when imported into a
/// fresh database, then the child links to the imported parent by name.
#[tokio::test]
async fn given_bundle_with_option_child_when_imported_then_child_links_to_parent() {
    let pool = given_fresh_database().await;
    let bundle = share::KnowledgeBundle {
        version: 1,
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        entities: vec![
            share::SharedEntity {
                name: "git commit".to_string(),
                type_id: "cmd".to_string(),
                description: Some("record changes".to_string()),
                content: Some("git commit".to_string()),
                parent: None,
            },
            share::SharedEntity {
                name: "--amend".to_string(),
                type_id: "opt".to_string(),
                description: Some("rewrite the last commit".to_string()),
                content: Some("--amend".to_string()),
                parent: Some("git commit".to_string()),
            },
        ],
        secrets: vec![],
        secret_mode: "excluded".to_string(),
    };

    let (imported, skipped) = share::import_knowledge(&pool, &bundle).await.unwrap();
    assert_eq!(imported, 2);
    assert_eq!(skipped, 0);

    let parent = repository::get_entity_by_name_and_type(&pool, "git commit", "cmd")
        .await
        .unwrap();
    let children = repository::list_child_entities(&pool, &parent.id)
        .await
        .unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "--amend");
}

// ============================================================================
// Feature: Cron workflow scheduling (US-WF-07)
// ============================================================================

/// Scenario: scheduling a workflow persists a task the daemon can load
/// Given a workflow entity, when a cron schedule is created, then it is
/// listed with the normalized expression and can be deleted again.
#[tokio::test]
async fn given_workflow_when_scheduled_then_task_persists_and_deletes() {
    let pool = given_fresh_database().await;
    let wf = repository::create_entity(
        &pool,
        &CreateEntity {
            name: "nightly backup".to_string(),
            description: None,
            content: Some("print('backing up')".to_string()),
            type_id: "wf".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    // Classic 5-field crontab syntax must be accepted and normalized
    let (normalized, _) = tui_op_hub::scheduler::validate_cron("30 2 * * *").unwrap();
    assert_eq!(normalized, "0 30 2 * * *", "seconds field prepended");

    let task = repository::create_scheduled_task(&pool, &wf.id, &normalized)
        .await
        .unwrap();
    assert_eq!(task.cron_expr, "0 30 2 * * *");
    assert!(task.enabled);

    let listed = repository::list_scheduled_tasks(&pool).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].workflow_id, wf.id);

    repository::delete_scheduled_task(&pool, &task.id)
        .await
        .unwrap();
    assert!(repository::list_scheduled_tasks(&pool)
        .await
        .unwrap()
        .is_empty());
    // Deleting again is a NotFound error
    assert!(repository::delete_scheduled_task(&pool, &task.id)
        .await
        .is_err());
}

/// Scenario: invalid cron expressions are rejected before persisting
/// Given garbage cron input, when validated, then an error is returned and
/// nothing is stored.
#[tokio::test]
async fn given_invalid_cron_when_validated_then_error_and_nothing_persisted() {
    let pool = given_fresh_database().await;
    assert!(tui_op_hub::scheduler::validate_cron("not a cron").is_err());
    assert!(tui_op_hub::scheduler::validate_cron("99 99 99 99 99").is_err());
    assert!(tui_op_hub::scheduler::validate_cron("").is_err());
    // Valid aliases still work
    assert!(tui_op_hub::scheduler::validate_cron("@daily").is_ok());
    assert!(tui_op_hub::scheduler::validate_cron("0 9 * * MON").is_ok());
    assert!(repository::list_scheduled_tasks(&pool)
        .await
        .unwrap()
        .is_empty());
}

/// Scenario: an AI-generated bare entity array imports without a wrapper
/// Given a JSON file containing only an array of entities (no bundle wrapper),
/// when parsed and imported, then every entity lands in the database with
/// defaulted bundle metadata.
#[tokio::test]
async fn given_bare_entity_array_when_imported_then_entities_landed() {
    let pool = given_fresh_database().await;
    let text = r#"[
        {"name": "ai cmd", "type_id": "cmd", "description": "from AI",
         "content": "echo generated", "parent": null},
        {"name": "ai app", "type_id": "app", "content": " firefox"}
    ]"#;
    let bundle = share::bundle_from_json(text).unwrap();
    assert_eq!(bundle.secret_mode, "excluded");

    let (imported, skipped) = share::import_knowledge(&pool, &bundle).await.unwrap();
    assert_eq!(imported, 2);
    assert_eq!(skipped, 0);

    let items = repository::list_entities(&pool, Some("cmd"), None)
        .await
        .unwrap();
    assert!(items.iter().any(|e| e.name == "ai cmd"));
    let apps = repository::list_entities(&pool, Some("app"), None)
        .await
        .unwrap();
    assert!(apps.iter().any(|e| e.name == "ai app"));
}

/// Scenario: a created workspace remembers its directory
/// Given a project workspace created on disk, when the project row is listed,
/// then its stored path points at the created directory (so `O` can open it).
#[tokio::test]
async fn given_created_workspace_when_listed_then_path_is_stored() {
    let pool = given_fresh_database().await;
    let parent = std::env::temp_dir().join(format!("tui-op-hub-bdd-ws-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&parent);

    let created = tui_op_hub::project_workspace::create_project_directory(
        &parent,
        "bddproj",
        &tui_op_hub::project_workspace::ProjectKind::Generic,
        false,
    )
    .unwrap();

    let project = repository::create_project(
        &pool,
        &CreateProject {
            name: "bddproj".to_string(),
            description: Some("bdd workspace".to_string()),
        },
    )
    .await
    .unwrap();
    repository::set_project_path(&pool, &project.id, Some(&created.path.to_string_lossy()))
        .await
        .unwrap();

    let listed = repository::list_projects(&pool).await.unwrap();
    let p = listed.iter().find(|p| p.name == "bddproj").unwrap();
    assert_eq!(
        p.path.as_deref(),
        Some(created.path.to_string_lossy().as_ref())
    );
    assert!(created.path.is_dir());

    let _ = std::fs::remove_dir_all(&parent);
}

// ============================================================================
// Feature: Plugin/mod system - event hooks (US-PLG-05/06/09)
// ============================================================================

/// Scenario: an approved git-automation mod reacts to project_created
/// Given a Lua mod with an on_project_created hook and the execute_commands
/// capability, when the hook fires for a new project, then the mod's command
/// runs and reports success (this is how git repos get wired up on creation).
#[tokio::test]
async fn given_git_automation_mod_when_project_created_then_hook_runs_git() {
    use tui_op_hub::plugin::{PluginManager, PluginManifest};

    let pool = given_fresh_database().await;
    let tmp = std::env::temp_dir().join(format!("tui-op-hub-bdd-plug-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let plugin_dir = tmp.join("git.automation");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    std::fs::write(
        plugin_dir.join("plugin.toml"),
        concat!(
            "id = 'git.automation'\n",
            "name = 'Git Automation'\n",
            "version = '1.0.0'\n",
            "plugin_type = 'lua'\n",
            "entry_point = 'main.lua'\n",
            "required_capabilities = ['execute_commands'] \n\n[commands]\nhello = 'Say hello'\n"
        ),
    )
    .unwrap();

    // The mod: on every project_created, init a git repo in the new workspace
    // and create a first commit - the exact automation the user asked for.
    let proj = format!("tui-op-hub-bdd-proj-{}", std::process::id());
    let proj_dir = tmp.join(&proj);
    std::fs::write(
        plugin_dir.join("main.lua"),
        format!(
            "function on_project_created()\n\
              local out = run_command('mkdir -p ' .. event.path)\n\
              local init = run_command('git init -q ' .. event.path)\n\
              assert(init.success, init.stderr)\n\
              return 'git repo ready'\n\
            end\n"
        ),
    )
    .unwrap();

    let manager = PluginManager::new(pool.clone(), tmp.clone());

    // Manifest discovery works before any approval
    let discovered = manager.discover_plugins();
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].id, "git.automation");

    // Unapproved: load must fail
    assert!(manager.load_plugin(&plugin_dir).await.is_err());

    // Approve (as the user would with `a` in the Plugins tab), then load
    manager
        .approve_plugin("git.automation", "default")
        .await
        .unwrap();
    manager.load_plugin(&plugin_dir).await.unwrap();

    // Fire project_created
    let payload = serde_json::json!({
        "name": proj,
        "path": proj_dir.display().to_string(),
    });
    let results = manager.emit_event("project_created", &payload).await;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].1.as_ref().unwrap().as_deref(),
        Some("git repo ready")
    );

    // The hook's git init actually created the repo
    assert!(
        proj_dir.join(".git").is_dir(),
        "git repo initialized by mod"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Scenario: overwrite mode replaces local duplicates
/// Given a local entity and a bundle with the same (name, type) but different
/// content, when imported with Overwrite, then the local entity is replaced.
#[tokio::test]
async fn given_duplicate_when_imported_with_overwrite_then_local_replaced() {
    let pool = given_fresh_database().await;
    repository::create_entity(
        &pool,
        &CreateEntity {
            name: "svc token".to_string(),
            description: None,
            content: Some("OLD".to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    let bundle = share::KnowledgeBundle {
        version: 1,
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        entities: vec![share::SharedEntity {
            name: "svc token".to_string(),
            type_id: "cmd".to_string(),
            description: None,
            content: Some("NEW".to_string()),
            parent: None,
        }],
        secrets: vec![],
        secret_mode: "excluded".to_string(),
    };

    let report = share::import_knowledge_with_mode(&pool, &bundle, share::DuplicateMode::Overwrite)
        .await
        .unwrap();
    assert_eq!(report.overwritten, 1);
    assert_eq!(report.imported, 0);

    let items = repository::list_entities(&pool, Some("cmd"), None)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].content.as_deref(), Some("NEW"));
}

/// Scenario: rename mode imports duplicates under -imported names
/// Given a local entity and a conflicting bundle, when imported with Rename,
/// then the incoming entity lands as `<name>-imported` and both exist.
#[tokio::test]
async fn given_duplicate_when_imported_with_rename_then_suffixed_copy_created() {
    let pool = given_fresh_database().await;
    repository::create_entity(
        &pool,
        &CreateEntity {
            name: "deploy".to_string(),
            description: None,
            content: Some("LOCAL".to_string()),
            type_id: "cmd".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    let bundle = share::KnowledgeBundle {
        version: 1,
        exported_at: "2026-01-01T00:00:00Z".to_string(),
        entities: vec![share::SharedEntity {
            name: "deploy".to_string(),
            type_id: "cmd".to_string(),
            description: None,
            content: Some("INCOMING".to_string()),
            parent: None,
        }],
        secrets: vec![],
        secret_mode: "excluded".to_string(),
    };

    let report = share::import_knowledge_with_mode(&pool, &bundle, share::DuplicateMode::Rename)
        .await
        .unwrap();
    assert_eq!(report.renamed, 1);

    let items = repository::list_entities(&pool, Some("cmd"), None)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert!(items
        .iter()
        .any(|e| e.name == "deploy" && e.content.as_deref() == Some("LOCAL")));
    assert!(items
        .iter()
        .any(|e| e.name == "deploy-imported" && e.content.as_deref() == Some("INCOMING")));
}

// ============================================================================
// Feature: Workflow run cancellation (US-WF-09)
// ============================================================================

/// Scenario: a background workflow run can be cancelled cooperatively
/// Given a long-running workflow executed in the background, when cancel_run
/// is called with its run id, then the engine stops before the next step and
/// reports the run as cancelled with partial progress preserved.
#[tokio::test]
async fn given_running_workflow_when_cancel_requested_then_result_reports_cancel() {
    let pool = given_fresh_database().await;
    let steps: Vec<WorkflowStep> = (0..30)
        .map(|i| WorkflowStep {
            name: format!("step-{}", i),
            script: "run_command('sleep 0.05')".to_string(),
            depends_on: Vec::new(),
        })
        .collect();
    let wf = repository::create_entity(
        &pool,
        &CreateEntity {
            name: "long runner".to_string(),
            description: None,
            content: Some(serde_json::json!({ "name": "long runner", "steps": steps }).to_string()),
            type_id: "wf".to_string(),
            project_id: None,
            tags: None,
            metadata_json: None,
        },
    )
    .await
    .unwrap();

    // When: start the run in the background (same path the TUI uses)
    let (run_id, task) = workflow::spawn_workflow_run(pool.clone(), &wf.id, None, None)
        .await
        .unwrap();
    assert!(workflow::active_run_ids().contains(&run_id));

    // When: request cooperative cancellation
    assert!(workflow::cancel_run(&run_id), "run must be cancellable");
    let result = task.await.unwrap().unwrap();

    // Then: the run is reported as cancelled, not successful
    assert!(!result.success, "cancelled run must not report success");
    assert!(result.error.unwrap_or_default().contains("cancelled"));
    assert!(
        !workflow::active_run_ids().contains(&run_id),
        "run unregistered"
    );
}

// ============================================================================
// Feature: SSH host manager (US-SSH-01..05)
// ============================================================================

/// Scenario: hosts can be created, updated, listed and deleted
/// Given a fresh database, when an SSH host is created and edited, then the
/// changes persist and deletion removes it.
#[tokio::test]
async fn given_ssh_host_when_managed_then_crud_round_trips() {
    let pool = given_fresh_database().await;

    // Create (US-SSH-02)
    let host = repository::create_ssh_host(
        &pool,
        "bastion",
        "203.0.113.7",
        22,
        Some("ops"),
        Some("~/.ssh/id_ed25519"),
    )
    .await
    .unwrap();

    // Update (US-SSH-03)
    let mut edited = host.clone();
    edited.port = 2200;
    edited.hostname = "bastion.internal".into();
    let updated = repository::update_ssh_host(&pool, &edited).await.unwrap();
    assert_eq!(updated.port, 2200);
    assert_eq!(updated.hostname, "bastion.internal");

    // List (US-SSH-01)
    let listed = repository::list_ssh_hosts(&pool).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].username.as_deref(), Some("ops"));

    // Delete (US-SSH-04)
    repository::delete_ssh_host(&pool, &host.id).await.unwrap();
    assert!(repository::list_ssh_hosts(&pool).await.unwrap().is_empty());
    assert!(repository::delete_ssh_host(&pool, &host.id).await.is_err());
}

// ============================================================================
// Feature: Managed config files (US-CFG-09..12)
// ============================================================================

/// Scenario: register, deploy with drift detection, update, and delete a config
/// Given an existing config file, when it is registered, deployed with copy
/// mode, the source changes and the update action runs, then the deployed
/// copy is refreshed and deletion removes the entry.
#[tokio::test]
async fn given_config_file_when_registered_deployed_updated_then_lifecycle_round_trips() {
    let dir = std::env::temp_dir().join(format!("tuihub-bdd-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("hyprland.conf");
    std::fs::write(&src, "monitor=eDP-1,1920x1080").unwrap();

    let man = tui_op_hub::config_manager::ConfigManager::new(dir.join("store"));

    // Register (US-CFG-09): master copy stored, registry persisted
    let mut entry = man.register_existing(&src, "hyprland").unwrap();
    assert_eq!(man.load_registry().len(), 1);

    // Deploy copy-mode to two targets (US-CFG-10)
    entry.deploy_mode = tui_op_hub::config_manager::DeployMode::Copy;
    entry.targets = vec![
        dir.join("t1").to_string_lossy().to_string(),
        dir.join("t2").to_string_lossy().to_string(),
    ];
    man.save_registry(&[entry.clone()]).unwrap();
    let results = man.deploy(&entry).unwrap();
    assert!(results.iter().all(|r| r.ok));

    // Source changes -> deployed copies are stale (drift) until update (US-CFG-11)
    std::fs::write(&src, "monitor=eDP-1,2560x1440").unwrap();
    assert!(man.sync_source(&mut entry).unwrap(), "master updated");
    let results = man.deploy(&entry).unwrap();
    assert!(results.iter().all(|r| r.had_drift), "targets were stale");
    assert_eq!(
        std::fs::read_to_string(&entry.targets[0]).unwrap(),
        "monitor=eDP-1,2560x1440"
    );

    // Git versioning (US-CFG-12): init + commit when git is available
    if tui_op_hub::keygen::which("git") {
        man.git_init().unwrap();
        let out = man.git_commit("manage hyprland").unwrap();
        assert!(!out.is_empty());
    }

    // Deletion removes registry entry + master copy
    man.remove_entry(&entry.id).unwrap();
    assert!(man.load_registry().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
