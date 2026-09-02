//! List state management for TUI views
//!
//! Provides generic list state with selection, pagination, search, and filtering

use crate::models::*;
use crate::workflow::{WorkflowDefinition, WorkflowStep};
use std::collections::HashMap;

/// Generic list state with pagination and selection
#[derive(Debug, Clone)]
pub struct ListState<T> {
    pub items: Vec<T>,
    pub selected: usize,
    pub page: usize,
    pub page_size: usize,
    pub search_query: String,
    pub filter: Option<String>,
    pub total_count: usize,
}

impl<T> Default for ListState<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            page: 0,
            page_size: 10,
            search_query: String::new(),
            filter: None,
            total_count: 0,
        }
    }
}

impl<T> ListState<T> {
    pub fn new(page_size: usize) -> Self {
        Self {
            page_size,
            ..Default::default()
        }
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn next_page(&mut self) {
        let max_page = (self.total_count.saturating_sub(1)) / self.page_size;
        if self.page < max_page {
            self.page += 1;
            self.selected = 0;
        }
    }

    pub fn previous_page(&mut self) {
        if self.page > 0 {
            self.page -= 1;
            self.selected = 0;
        }
    }

    pub fn get_selected(&self) -> Option<&T> {
        self.items.get(self.selected)
    }

    pub fn set_items(&mut self, items: Vec<T>, total: usize) {
        self.items = items;
        self.total_count = total;
        if self.selected >= self.items.len() && !self.items.is_empty() {
            self.selected = self.items.len() - 1;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selected = 0;
        self.total_count = 0;
    }
}

/// Commands list state
pub type CommandsListState = ListState<Entity>;

/// Projects list state  
pub type ProjectsListState = ListState<Project>;

/// Workflows list state
pub type WorkflowsListState = ListState<Entity>;

/// Secrets list state
pub type SecretsListState = ListState<Secret>;

/// Form mode for create/edit operations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormMode {
    Create,
    Edit,
    View,
}

/// Command form state
#[derive(Debug, Clone, Default)]
pub struct CommandFormState {
    pub name: String,
    pub description: String,
    pub content: String,
    pub project_id: Option<String>,
    pub tags: Vec<String>,
    pub focused_field: usize,
    pub mode: Option<FormMode>,
    /// Entity being edited (None = create new)
    pub editing_id: Option<String>,
    /// Index into ENTITY_TYPE_IDS for the type selector (cmd/script/app)
    pub entity_type: usize,
    pub tags_text: String,
    pub error_message: Option<String>,
}

/// Base entity types selectable in the command form (US-CMD-01, US-CMD-06)
pub const ENTITY_TYPE_IDS: [&str; 3] = ["cmd", "script", "app"];
pub const ENTITY_TYPE_NAMES: [&str; 3] = ["Command", "Script", "App"];
/// Fields: Name, Type, Description, Content, Tags
pub const COMMAND_FORM_FIELDS: usize = 5;

/// Project form state
#[derive(Debug, Clone, Default)]
pub struct ProjectFormState {
    pub name: String,
    pub description: String,
    pub focused_field: usize,
    pub mode: Option<FormMode>,
    pub editing_id: Option<String>,
    pub error_message: Option<String>,
}

/// Fields: Name, Description
pub const PROJECT_FORM_FIELDS: usize = 2;

/// Workflow form state
#[derive(Debug, Clone, Default)]
pub struct WorkflowFormState {
    pub name: String,
    pub description: String,
    pub content: String,
    pub project_id: Option<String>,
    pub focused_field: usize,
    pub mode: Option<FormMode>,
    pub editing_id: Option<String>,
    pub error_message: Option<String>,
}

/// Fields: Name, Description, Script
pub const WORKFLOW_FORM_FIELDS: usize = 3;

/// Secret form state
#[derive(Debug, Clone, Default)]
pub struct SecretFormState {
    pub name: String,
    pub value: String,
    pub focused_field: usize,
    pub mode: Option<FormMode>,
    pub editing_id: Option<String>,
    pub error_message: Option<String>,
}

/// Fields: Name, Value
pub const SECRET_FORM_FIELDS: usize = 2;

/// A single step in the visual workflow builder, backed by a saved command/script entity.
#[derive(Debug, Clone)]
pub struct VisualStep {
    pub entity_id: String,
    pub name: String,
    pub script: String,
}

/// Focused area of the visual workflow builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualField {
    #[default]
    Name,
    Description,
    Steps,
}

/// Command picker overlay (choose from saved cmd/script entities).
#[derive(Debug, Clone, Default)]
pub struct CommandPickerState {
    pub items: Vec<Entity>,
    pub selected: usize,
}

impl CommandPickerState {
    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1).min(self.items.len() - 1);
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn get_selected(&self) -> Option<&Entity> {
        self.items.get(self.selected)
    }
}

/// Visual workflow builder state (US-WF-03, US-WF-10): compose workflows
/// from saved commands instead of writing Lua by hand.
#[derive(Debug, Clone, Default)]
pub struct VisualWorkflowState {
    pub name: String,
    pub description: String,
    pub steps: Vec<VisualStep>,
    pub focused_field: VisualField,
    pub selected_step: usize,
    pub picker: Option<CommandPickerState>,
    pub editing_id: Option<String>,
    pub error_message: Option<String>,
}

impl VisualWorkflowState {
    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            VisualField::Name => VisualField::Description,
            VisualField::Description => VisualField::Steps,
            VisualField::Steps => VisualField::Name,
        };
    }

    pub fn previous_field(&mut self) {
        self.focused_field = match self.focused_field {
            VisualField::Name => VisualField::Steps,
            VisualField::Description => VisualField::Name,
            VisualField::Steps => VisualField::Description,
        };
    }

    pub fn select_step_next(&mut self) {
        if !self.steps.is_empty() {
            self.selected_step = (self.selected_step + 1).min(self.steps.len() - 1);
        }
    }

    pub fn select_step_previous(&mut self) {
        if self.selected_step > 0 {
            self.selected_step -= 1;
        }
    }

    /// Remove the selected step and keep the selection in bounds.
    pub fn remove_selected_step(&mut self) {
        if self.selected_step < self.steps.len() {
            self.steps.remove(self.selected_step);
            if self.selected_step >= self.steps.len() && !self.steps.is_empty() {
                self.selected_step = self.steps.len() - 1;
            }
        }
    }

    /// Move the selected step up (earlier in execution order).
    pub fn move_step_up(&mut self) {
        if self.selected_step > 0 && self.selected_step < self.steps.len() {
            self.steps.swap(self.selected_step, self.selected_step - 1);
            self.selected_step -= 1;
        }
    }

    /// Move the selected step down (later in execution order).
    pub fn move_step_down(&mut self) {
        if self.selected_step + 1 < self.steps.len() {
            self.steps.swap(self.selected_step, self.selected_step + 1);
            self.selected_step += 1;
        }
    }
}

// ── Settings screen state (US-APP-01, US-APP-02, US-APP-06) ─────────────────

/// Ordered Settings rows: editor, page size, theme preset, then the 9 keybinding actions.
pub const SETTINGS_ROWS: [&str; 12] = [
    "editor",
    "page_size",
    "theme",
    "quit",
    "help",
    "search",
    "filter",
    "create",
    "edit",
    "delete",
    "copy",
    "run",
];

/// Rows 3..=11 are keybinding actions; row 2 is the theme preset (cycles).
pub const SETTINGS_ACTIONS: [&str; 9] = [
    "quit", "help", "search", "filter", "create", "edit", "delete", "copy", "run",
];
pub const SETTINGS_KEYBIND_FIRST_ROW: usize = 3;
pub const SETTINGS_THEME_ROW: usize = 2;

/// Interaction state of the Settings screen.
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub selected: usize,
    /// Row whose text value is being edited (buffer holds the draft).
    pub editing_text: Option<usize>,
    /// Row capturing the next pressed key as a keybinding.
    pub capturing_key: Option<usize>,
    pub buffer: String,
    pub error: Option<String>,
    pub dirty: bool,
}

impl SettingsState {
    pub fn select_next(&mut self) {
        if self.selected + 1 < SETTINGS_ROWS.len() {
            self.selected += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// The action being rebind-captured, if any.
    pub fn capturing_action(&self) -> Option<&'static str> {
        self.capturing_key
            .and_then(|row| SETTINGS_ROWS.get(row))
            .copied()
    }
}

// ── Advanced settings screen (visual config, US-APP-01) ─────────────────────

/// Advanced rows: 11 theme colors, then database/API options.
pub const ADVANCED_ROWS: [&str; 14] = [
    "fg",
    "bg",
    "accent",
    "status_bg",
    "primary",
    "secondary",
    "success",
    "warning",
    "error",
    "border",
    "highlight",
    "db_path",
    "api_bind",
    "busy_timeout",
];

/// Rows `0..ADVANCED_COLOR_ROW_COUNT` are theme colors (visual swatch editor).
pub const ADVANCED_COLOR_ROW_COUNT: usize = 11;

/// Palette used by the `←`/`→` visual color cycling (named + hex entries).
pub const ADVANCED_PALETTE: [&str; 14] = [
    "black", "white", "red", "green", "yellow", "blue", "magenta", "cyan", "gray", "darkgray",
    "#ff6600", "#3fb950", "#58a6ff", "#f85149",
];

/// True when the advanced row is a theme color (swatch + cycle editor).
pub fn is_advanced_color_row(row: usize) -> bool {
    row < ADVANCED_COLOR_ROW_COUNT
}

/// True when the advanced row is an optional theme override (clearable).
pub fn is_advanced_optional_color(row: usize) -> bool {
    row >= 4 && is_advanced_color_row(row) // primary…highlight
}

// ── SSH / GPG key generation form (US-SEC-01) ───────────────────────────────

/// Key kinds selectable in the keygen form.
pub const KEYGEN_KINDS: [&str; 2] = ["ssh", "gpg"];
/// Fields: 0=Name, 1=Email (gpg only), 2=Passphrase, 3=Kind.
pub const KEYGEN_FIELDS: usize = 4;

/// State of the SSH/GPG key generation form (opened with `k` on Secrets).
#[derive(Debug, Clone, Default)]
pub struct KeygenState {
    pub open: bool,
    pub name: String,
    pub email: String,
    pub passphrase: String,
    pub kind: usize,
    pub focused_field: usize,
    pub error: Option<String>,
}

impl KeygenState {
    pub fn select_next_field(&mut self) {
        self.focused_field = (self.focused_field + 1) % KEYGEN_FIELDS;
    }

    pub fn select_previous_field(&mut self) {
        self.focused_field = (self.focused_field + KEYGEN_FIELDS - 1) % KEYGEN_FIELDS;
    }

    pub fn kind_name(&self) -> &'static str {
        KEYGEN_KINDS.get(self.kind).copied().unwrap_or("ssh")
    }
}

/// Advanced settings sub-screen: visual theme color editor (swatches, live
/// `←`/`→` palette cycling, hex text editing) plus database/API options.
/// Opened from the Settings screen with `a`.
#[derive(Debug, Clone, Default)]
pub struct AdvancedState {
    pub active: bool,
    pub selected: usize,
    /// A text value is being edited (buffer holds the draft).
    pub editing: bool,
    pub buffer: String,
    pub error: Option<String>,
}

impl AdvancedState {
    pub fn select_next(&mut self) {
        if self.selected + 1 < ADVANCED_ROWS.len() {
            self.selected += 1;
        }
    }

    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn row_name(&self) -> &'static str {
        ADVANCED_ROWS.get(self.selected).copied().unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(id: &str, name: &str) -> VisualStep {
        VisualStep {
            entity_id: id.to_string(),
            name: name.to_string(),
            script: format!("echo {}", name),
        }
    }

    // ── Generic ListState (pagination, selection) ───────────────────────────

    #[test]
    fn list_state_navigates_selection_within_bounds() {
        let mut list: ListState<i32> = ListState::new(2);
        assert!(list.get_selected().is_none(), "empty list has no selection");

        list.set_items(vec![1, 2, 3], 3);
        assert_eq!(list.get_selected(), Some(&1));

        list.select_next();
        list.select_next();
        assert_eq!(list.get_selected(), Some(&3));
        list.select_next(); // clamped at the end
        assert_eq!(list.get_selected(), Some(&3));

        list.select_previous();
        list.select_previous();
        list.select_previous();
        assert_eq!(list.get_selected(), Some(&1));
        list.select_previous(); // clamped at the start
        assert_eq!(list.get_selected(), Some(&1));
    }

    #[test]
    fn list_state_set_items_keeps_selection_in_bounds() {
        let mut list: ListState<i32> = ListState::new(2);
        list.set_items(vec![1, 2, 3], 3);
        list.selected = 2;

        // Shrinking the list pulls the selection back into bounds
        list.set_items(vec![1], 1);
        assert_eq!(list.selected, 0);
    }

    #[test]
    fn list_state_pagination_is_bounded_by_total_count() {
        let mut list: ListState<i32> = ListState::new(2);
        list.set_items(vec![1, 2, 3, 4, 5], 5);

        list.previous_page(); // already at first page
        assert_eq!(list.page, 0);

        list.next_page();
        list.next_page(); // pages: 0,1,2 for 5 items @ size 2
        assert_eq!(list.page, 2);
        list.next_page(); // clamped
        assert_eq!(list.page, 2);

        list.previous_page();
        assert_eq!(list.page, 1);
    }

    // ── Command picker ──────────────────────────────────────────────────────

    #[test]
    fn picker_navigation_is_clamped() {
        let mut picker = CommandPickerState {
            items: vec![Entity {
                id: "1".into(),
                name: "cmd".into(),
                description: None,
                content: None,
                type_id: "cmd".into(),
                project_id: None,
                metadata_json: None,
                created_at: String::new(),
                updated_at: String::new(),
            }],
            selected: 0,
        };
        picker.select_next(); // clamped at last item
        assert_eq!(picker.selected, 0);
        picker.select_previous(); // already at first
        assert_eq!(picker.selected, 0);
        assert!(picker.get_selected().is_some());
    }

    // ── Visual workflow builder (BDD style) ─────────────────────────────────

    /// Scenario: compose a workflow from saved commands
    /// Given an empty builder, when steps are added, then they keep insertion order.
    #[test]
    fn given_empty_builder_when_steps_added_then_order_matches_insertion() {
        let mut builder = VisualWorkflowState::default();
        assert!(builder.steps.is_empty());

        builder.steps.push(step("a", "first"));
        builder.steps.push(step("b", "second"));
        builder.steps.push(step("c", "third"));

        let names: Vec<&str> = builder.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    /// Scenario: reorder a step down
    /// Given a builder with 3 steps, when the middle step moves down, then it becomes last.
    #[test]
    fn given_three_steps_when_middle_moves_down_then_order_updates() {
        let mut builder = VisualWorkflowState::default();
        builder.steps = vec![step("a", "one"), step("b", "two"), step("c", "three")];
        builder.selected_step = 1;

        builder.move_step_down();
        assert_eq!(builder.selected_step, 2);
        let names: Vec<&str> = builder.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["one", "three", "two"]);

        builder.move_step_down(); // already last — no-op
        assert_eq!(builder.selected_step, 2);
        assert_eq!(builder.steps.len(), 3);
    }

    /// Scenario: reorder a step up
    /// Given a builder with 3 steps, when the last step moves up twice, then it becomes first.
    #[test]
    fn given_three_steps_when_last_moves_up_then_becomes_first() {
        let mut builder = VisualWorkflowState::default();
        builder.steps = vec![step("a", "one"), step("b", "two"), step("c", "three")];
        builder.selected_step = 2;

        builder.move_step_up();
        builder.move_step_up();
        assert_eq!(builder.selected_step, 0);
        let names: Vec<&str> = builder.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["three", "one", "two"]);

        builder.move_step_up(); // already first — no-op
        assert_eq!(builder.selected_step, 0);
    }

    /// Scenario: remove a step
    /// Given a builder with steps, when the selected step is removed, then the
    /// selection stays in bounds and the remaining order is preserved.
    #[test]
    fn given_steps_when_selected_removed_then_selection_stays_in_bounds() {
        let mut builder = VisualWorkflowState::default();
        builder.steps = vec![step("a", "one"), step("b", "two"), step("c", "three")];
        builder.selected_step = 2;

        builder.remove_selected_step();
        assert_eq!(builder.selected_step, 1); // clamped to new last
        let names: Vec<&str> = builder.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["one", "two"]);

        builder.remove_selected_step();
        builder.remove_selected_step();
        assert!(builder.steps.is_empty());
        builder.remove_selected_step(); // no-op on empty
        assert_eq!(builder.selected_step, 0);
    }

    /// Scenario: cycle the focused field
    /// Given the builder, when fields are cycled, then it wraps Name → Description → Steps.
    #[test]
    fn given_builder_when_fields_cycled_then_wraps_around() {
        let mut builder = VisualWorkflowState::default();
        assert_eq!(builder.focused_field, VisualField::Name);

        builder.next_field();
        assert_eq!(builder.focused_field, VisualField::Description);
        builder.next_field();
        assert_eq!(builder.focused_field, VisualField::Steps);
        builder.next_field();
        assert_eq!(builder.focused_field, VisualField::Name);

        builder.previous_field();
        assert_eq!(builder.focused_field, VisualField::Steps);
    }

    // ── Pure helpers ────────────────────────────────────────────────────────

    #[test]
    fn parse_tags_text_trims_and_drops_empties() {
        assert_eq!(
            parse_tags_text("git, docker , , network"),
            vec!["git", "docker", "network"]
        );
        assert!(parse_tags_text("  , , ").is_empty());
        assert!(parse_tags_text("").is_empty());
    }

    /// Scenario: save a visual workflow
    /// Given steps from saved commands, when the definition is built and
    /// serialized, then it round-trips back through serde into the same steps.
    #[test]
    fn given_visual_steps_when_definition_built_then_json_round_trips() {
        let steps = vec![step("a", "backup"), step("b", "cleanup")];
        let definition = build_workflow_definition("nightly", "backup then clean", &steps).unwrap();

        assert_eq!(definition.name, "nightly");
        assert_eq!(definition.description.as_deref(), Some("backup then clean"));
        assert_eq!(definition.steps.len(), 2);
        assert_eq!(definition.steps[0].name, "backup");
        assert_eq!(definition.steps[1].script, "echo cleanup");

        let json = serde_json::to_string(&definition).unwrap();
        let parsed: WorkflowDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.steps.len(), 2);
        assert_eq!(parsed.steps[1].name, "cleanup");
    }

    #[test]
    fn build_workflow_definition_rejects_empty_name_and_steps() {
        let err = build_workflow_definition("  ", "d", &[step("a", "x")]).unwrap_err();
        assert_eq!(err, "Name is required");

        let err = build_workflow_definition("name", "d", &[]).unwrap_err();
        assert!(err.contains("at least one step"));
    }

    #[test]
    fn entity_type_constants_match_seed_ids() {
        // 0001_init.sql seeds these exact type ids
        assert_eq!(ENTITY_TYPE_IDS, ["cmd", "script", "app"]);
        assert_eq!(ENTITY_TYPE_IDS.len(), ENTITY_TYPE_NAMES.len());
        assert_eq!(COMMAND_FORM_FIELDS, 5);
        assert_eq!(PROJECT_FORM_FIELDS, 2);
        assert_eq!(WORKFLOW_FORM_FIELDS, 3);
        assert_eq!(SECRET_FORM_FIELDS, 2);
    }

    // ── Advanced settings state (visual config) ─────────────────────────────

    #[test]
    fn advanced_state_navigation_clamps() {
        let mut adv = AdvancedState::default();
        adv.select_previous(); // clamp at top
        assert_eq!(adv.selected, 0);
        for _ in 0..(ADVANCED_ROWS.len() + 5) {
            adv.select_next();
        }
        assert_eq!(adv.selected, ADVANCED_ROWS.len() - 1, "clamped at bottom");
        adv.select_next(); // still clamped
        assert_eq!(adv.selected, ADVANCED_ROWS.len() - 1);
        assert_eq!(adv.row_name(), "busy_timeout");
    }

    #[test]
    fn advanced_rows_split_colors_and_system_options() {
        assert_eq!(ADVANCED_ROWS.len(), 14);
        assert!(is_advanced_color_row(0), "fg is a color row");
        assert!(is_advanced_color_row(ADVANCED_COLOR_ROW_COUNT - 1));
        assert!(!is_advanced_color_row(ADVANCED_COLOR_ROW_COUNT));
        assert_eq!(ADVANCED_ROWS[ADVANCED_COLOR_ROW_COUNT], "db_path");

        // primary…highlight are optional (clearable back to preset)
        assert!(!is_advanced_optional_color(0), "fg is not optional");
        assert!(is_advanced_optional_color(4), "primary is optional");
        assert!(is_advanced_optional_color(10), "highlight is optional");
        assert!(!is_advanced_optional_color(11), "db_path is not a color");

        // Palette must contain at least one hex and one named color
        assert!(ADVANCED_PALETTE.iter().any(|c| c.starts_with('#')));
        assert!(ADVANCED_PALETTE.iter().any(|c| !c.starts_with('#')));
    }
}

/// Search/Filter state
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub active: bool,
}

/// Parse a comma-separated tags text field into a clean tag list (US-CMD-04).
pub fn parse_tags_text(text: &str) -> Vec<String> {
    text.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Build a `WorkflowDefinition` from visual builder state (US-WF-03, US-WF-10).
///
/// The definition serializes to JSON which is stored as the workflow entity
/// content; the Lua engine executes the steps in definition order.
pub fn build_workflow_definition(
    name: &str,
    description: &str,
    steps: &[VisualStep],
) -> Result<WorkflowDefinition, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name is required".to_string());
    }
    if steps.is_empty() {
        return Err("Add at least one step (pick a saved command)".to_string());
    }
    Ok(WorkflowDefinition {
        name: name.to_string(),
        description: if description.trim().is_empty() {
            None
        } else {
            Some(description.trim().to_string())
        },
        steps: steps
            .iter()
            .map(|s| WorkflowStep {
                name: s.name.clone(),
                script: s.script.clone(),
                depends_on: vec![],
            })
            .collect(),
        variables: HashMap::new(),
    })
}

impl SearchState {
    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn clear(&mut self) {
        self.query.clear();
    }
}
