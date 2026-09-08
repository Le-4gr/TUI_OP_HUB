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

/// Base entity types selectable in the command form (US-CMD-01, US-CMD-06,
/// `chain` = pipe/semicolon command chains, US-CMD chains)
pub const ENTITY_TYPE_IDS: [&str; 4] = ["cmd", "script", "app", "chain"];
pub const ENTITY_TYPE_NAMES: [&str; 4] = ["Command", "Script", "App", "Chain"];
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
    /// Group label, e.g. `github`, `servers` (US-SEC).
    pub group: String,
    pub username: String,
    pub url: String,
    pub email: String,
    /// When non-empty, the value is wrapped with this passphrase on save.
    pub passphrase: String,
    /// Offer the secret (SSH keys) to ssh-agent.
    pub ssh_agent: bool,
    pub focused_field: usize,
    pub mode: Option<FormMode>,
    pub editing_id: Option<String>,
    pub error_message: Option<String>,
}

/// Fields: Name, Value, Group, Username, URL, Email, Passphrase, SSH-agent
pub const SECRET_FORM_FIELDS: usize = 8;

/// Boolean/comparison operator for Compare nodes (US-FUT-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareOp {
    #[default]
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CompareOp {
    pub fn all() -> &'static [CompareOp] {
        &[
            CompareOp::Eq,
            CompareOp::Ne,
            CompareOp::Lt,
            CompareOp::Le,
            CompareOp::Gt,
            CompareOp::Ge,
        ]
    }

    /// Lua operator symbol.
    pub fn symbol(&self) -> &'static str {
        match self {
            CompareOp::Eq => "==",
            CompareOp::Ne => "~=",
            CompareOp::Lt => "<",
            CompareOp::Le => "<=",
            CompareOp::Gt => ">",
            CompareOp::Ge => ">=",
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|o| o == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }
}

/// Node kind in the visual builder (US-FUT-07). `Command` is the classic
/// saved-command step; the rest are logic nodes with typed boolean ports:
/// their inputs read previous step results (`results["<step>"]`) and their
/// output is a boolean stored under the node's own name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualNodeKind {
    #[default]
    Command,
    And,
    Or,
    Not,
    Xor,
    Compare,
    IfElse,
}

impl VisualNodeKind {
    pub fn all() -> &'static [VisualNodeKind] {
        &[
            VisualNodeKind::Command,
            VisualNodeKind::And,
            VisualNodeKind::Or,
            VisualNodeKind::Not,
            VisualNodeKind::Xor,
            VisualNodeKind::Compare,
            VisualNodeKind::IfElse,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            VisualNodeKind::Command => "CMD",
            VisualNodeKind::And => "AND",
            VisualNodeKind::Or => "OR",
            VisualNodeKind::Not => "NOT",
            VisualNodeKind::Xor => "XOR",
            VisualNodeKind::Compare => "CMP",
            VisualNodeKind::IfElse => "IF/ELSE",
        }
    }

    pub fn next(&self) -> Self {
        let all = Self::all();
        let i = all.iter().position(|k| k == self).unwrap_or(0);
        all[(i + 1) % all.len()]
    }
}

/// A single step in the visual workflow builder, backed by a saved command/script entity.
#[derive(Debug, Clone)]
pub struct VisualStep {
    pub entity_id: String,
    pub name: String,
    pub script: String,
    /// Node kind (US-FUT-07); Command = plain saved step.
    pub kind: VisualNodeKind,
    /// Compare operator (Compare nodes).
    pub op: CompareOp,
    /// Gate inputs: names of earlier steps feeding AND/OR/NOT/XOR.
    pub inputs: Vec<String>,
    /// Compare/IfElse operands (left = then-branch for IfElse).
    pub left: String,
    pub right: String,
    /// IfElse condition (Lua expression over `results` / `ctx`).
    pub cond: String,
    /// Skip this step unless the Lua expression is truthy (US-FUT-07).
    pub run_when: Option<String>,
}

impl VisualStep {
    /// A plain command step backed by a saved entity.
    pub fn command(entity_id: &str, name: &str, script: &str) -> Self {
        Self {
            entity_id: entity_id.to_string(),
            name: name.to_string(),
            script: script.to_string(),
            kind: VisualNodeKind::Command,
            op: CompareOp::default(),
            inputs: Vec::new(),
            left: String::new(),
            right: String::new(),
            cond: String::new(),
            run_when: None,
        }
    }

    /// A fresh logic node (US-FUT-07).
    pub fn logic(kind: VisualNodeKind, name: &str) -> Self {
        Self {
            entity_id: String::new(),
            name: name.to_string(),
            script: String::new(),
            kind,
            op: CompareOp::default(),
            inputs: Vec::new(),
            left: String::new(),
            right: String::new(),
            cond: String::new(),
            run_when: None,
        }
    }

    /// One-line summary for the steps list.
    pub fn summary(&self) -> String {
        match self.kind {
            VisualNodeKind::Command => self.script.clone(),
            VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => {
                format!("{}({})", self.kind.label(), self.inputs.join(", "))
            }
            VisualNodeKind::Not => format!("NOT({})", self.inputs.join(", ")),
            VisualNodeKind::Compare => format!("{} {} {}", self.left, self.op.symbol(), self.right),
            VisualNodeKind::IfElse => {
                format!("if {} then {} else {}", self.cond, self.left, self.right)
            }
        }
    }
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

/// Inline editor for a logic node / step gate (US-FUT-07). One text row per
/// field depending on the node kind; ←/→ cycles Kind and the compare op.
#[derive(Debug, Clone)]
pub struct LogicNodeEditor {
    pub step_idx: usize,
    pub kind: VisualNodeKind,
    pub inputs: String,
    pub left: String,
    pub right: String,
    pub op: CompareOp,
    pub cond: String,
    pub run_when: String,
    pub focused: usize,
    pub error: Option<String>,
}

impl LogicNodeEditor {
    pub fn load(step_idx: usize, step: &VisualStep) -> Self {
        Self {
            step_idx,
            kind: step.kind,
            inputs: step.inputs.join(", "),
            left: step.left.clone(),
            right: step.right.clone(),
            op: step.op,
            cond: step.cond.clone(),
            run_when: step.run_when.clone().unwrap_or_default(),
            focused: 0,
            error: None,
        }
    }

    /// Field count after the Kind selector, by kind.
    pub fn field_count(&self) -> usize {
        match self.kind {
            VisualNodeKind::Command => 1,
            VisualNodeKind::And
            | VisualNodeKind::Or
            | VisualNodeKind::Not
            | VisualNodeKind::Xor => 2,
            VisualNodeKind::Compare | VisualNodeKind::IfElse => 4,
        }
    }

    pub fn field_label(&self, field: usize) -> &'static str {
        match self.kind {
            VisualNodeKind::Command => "Run when (Lua expr)",
            VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => match field {
                0 => "Inputs (step names, comma-sep)",
                _ => "Run when (Lua expr)",
            },
            VisualNodeKind::Not => match field {
                0 => "Input (step name)",
                _ => "Run when (Lua expr)",
            },
            VisualNodeKind::Compare => match field {
                0 => "Left",
                1 => "Operator (←/→)",
                2 => "Right",
                _ => "Run when (Lua expr)",
            },
            VisualNodeKind::IfElse => match field {
                0 => "Condition (Lua expr)",
                1 => "Then (step name)",
                2 => "Else (step name)",
                _ => "Run when (Lua expr)",
            },
        }
    }

    pub fn field_value(&self, field: usize) -> String {
        match self.kind {
            VisualNodeKind::Command => self.run_when.clone(),
            VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => match field {
                0 => self.inputs.clone(),
                _ => self.run_when.clone(),
            },
            VisualNodeKind::Not => match field {
                0 => self.inputs.clone(),
                _ => self.run_when.clone(),
            },
            VisualNodeKind::Compare => match field {
                0 => self.left.clone(),
                1 => self.op.symbol().to_string(),
                2 => self.right.clone(),
                _ => self.run_when.clone(),
            },
            VisualNodeKind::IfElse => match field {
                0 => self.cond.clone(),
                1 => self.left.clone(),
                2 => self.right.clone(),
                _ => self.run_when.clone(),
            },
        }
    }

    pub fn push_char(&mut self, field: usize, c: char) {
        match self.kind {
            VisualNodeKind::Command => self.run_when.push(c),
            VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => match field {
                0 => self.inputs.push(c),
                _ => self.run_when.push(c),
            },
            VisualNodeKind::Not => match field {
                0 => self.inputs.push(c),
                _ => self.run_when.push(c),
            },
            VisualNodeKind::Compare => match field {
                0 => self.left.push(c),
                2 => self.right.push(c),
                _ => self.run_when.push(c),
            },
            VisualNodeKind::IfElse => match field {
                0 => self.cond.push(c),
                1 => self.left.push(c),
                2 => self.right.push(c),
                _ => self.run_when.push(c),
            },
        }
    }

    pub fn pop_char(&mut self, field: usize) {
        match self.kind {
            VisualNodeKind::Command => {
                self.run_when.pop();
            }
            VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => match field {
                0 => {
                    self.inputs.pop();
                }
                _ => {
                    self.run_when.pop();
                }
            },
            VisualNodeKind::Not => match field {
                0 => {
                    self.inputs.pop();
                }
                _ => {
                    self.run_when.pop();
                }
            },
            VisualNodeKind::Compare => match field {
                0 => {
                    self.left.pop();
                }
                2 => {
                    self.right.pop();
                }
                _ => {
                    self.run_when.pop();
                }
            },
            VisualNodeKind::IfElse => match field {
                0 => {
                    self.cond.pop();
                }
                1 => {
                    self.left.pop();
                }
                2 => {
                    self.right.pop();
                }
                _ => {
                    self.run_when.pop();
                }
            },
        }
    }

    /// Write the editor state back onto the step (US-FUT-07) with arity checks.
    pub fn apply_to_step(&self, step: &mut VisualStep) -> Result<(), String> {
        step.kind = self.kind;
        step.op = self.op;
        step.inputs = self
            .inputs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        step.left = self.left.trim().to_string();
        step.right = self.right.trim().to_string();
        step.cond = self.cond.trim().to_string();
        step.run_when = if self.run_when.trim().is_empty() {
            None
        } else {
            Some(self.run_when.trim().to_string())
        };
        match self.kind {
            VisualNodeKind::Xor if step.inputs.len() != 2 => {
                return Err("XOR needs exactly 2 inputs".to_string());
            }
            VisualNodeKind::Not if step.inputs.len() != 1 => {
                return Err("NOT needs exactly 1 input".to_string());
            }
            VisualNodeKind::And | VisualNodeKind::Or if step.inputs.is_empty() => {
                return Err(format!("{} needs at least 1 input", self.kind.label()));
            }
            VisualNodeKind::Compare if step.left.is_empty() || step.right.is_empty() => {
                return Err("Comparison needs left and right operands".to_string());
            }
            VisualNodeKind::IfElse if step.left.is_empty() || step.right.is_empty() => {
                return Err("If/Else needs then and else step names".to_string());
            }
            _ => {}
        }
        Ok(())
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
    /// Logic-node editor popup (US-FUT-07).
    pub node_editor: Option<LogicNodeEditor>,
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
        VisualStep::command(id, name, &format!("echo {}", name))
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
                parent_id: None,
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

    // ── Logic nodes (US-FUT-07) ─────────────────────────────────────────────

    #[test]
    fn given_logic_nodes_when_definition_built_then_lua_compiled_with_ports() {
        let mut steps = vec![step("e1", "build"), step("e2", "lint")];
        let mut and_node = VisualStep::logic(VisualNodeKind::And, "gate1");
        and_node.inputs = vec!["build".into(), "lint".into()];
        steps.push(and_node);
        let mut not_node = VisualStep::logic(VisualNodeKind::Not, "invert");
        not_node.inputs = vec!["gate1".into()];
        steps.push(not_node);

        let def = build_workflow_definition("wf", "", &steps).unwrap();
        assert!(def.steps[2].script.contains("__t(results[\"build\"])"));
        assert!(def.steps[2].script.contains(" and "));
        assert!(def.steps[2].script.contains("__t(results[\"lint\"])"));
        assert_eq!(
            def.steps[2].depends_on,
            vec!["build".to_string(), "lint".to_string()]
        );
        assert!(
            def.steps[3].script.contains("not __t(results[\"gate1\"])"),
            "NOT compiles over its input port: {}",
            def.steps[3].script
        );
    }

    #[test]
    fn given_logic_node_referencing_later_step_when_built_then_error() {
        let mut not_node = VisualStep::logic(VisualNodeKind::Not, "gate");
        not_node.inputs = vec!["later".into()];
        let later = step("e2", "later");
        let steps = vec![not_node, later];
        let err = build_workflow_definition("wf", "", &steps).unwrap_err();
        assert!(err.contains("not an earlier step"), "{}", err);
    }

    #[test]
    fn given_compare_and_ifelse_nodes_when_built_then_scripts_compile() {
        let mut cmp = VisualStep::logic(VisualNodeKind::Compare, "cmp");
        cmp.left = "5".into();
        cmp.op = CompareOp::Ge;
        cmp.right = "3".into();
        let notify = step("e1", "notify");
        let noop = step("e2", "noop");
        let mut pick = VisualStep::logic(VisualNodeKind::IfElse, "pick");
        pick.cond = "results[\"cmp\"]".into();
        pick.left = "notify".into();
        pick.right = "noop".into();
        let steps = vec![cmp, notify, noop, pick];

        let def = build_workflow_definition("wf", "", &steps).unwrap();
        assert!(
            def.steps[0].script.contains("return 5 >= 3"),
            "numeric operands compile as literals: {}",
            def.steps[0].script
        );
        assert!(
            def.steps[3]
                .script
                .contains("if results[\"cmp\"] then return results[\"notify\"] else return results[\"noop\"] end"),
            "if/else compiles to a branch over results: {}",
            def.steps[3].script
        );
    }

    #[test]
    fn given_step_with_run_when_when_built_then_gate_preserved() {
        let mut s = step("e1", "build");
        s.run_when = Some("results[\"prev\"] == true".into());
        let def = build_workflow_definition("wf", "", &[s]).unwrap();
        assert_eq!(
            def.steps[0].run_when.as_deref(),
            Some("results[\"prev\"] == true")
        );
    }

    #[test]
    fn given_xor_node_with_wrong_arity_when_applied_then_error() {
        let mut editor = LogicNodeEditor {
            step_idx: 0,
            kind: VisualNodeKind::Xor,
            inputs: "a".into(),
            left: String::new(),
            right: String::new(),
            op: CompareOp::default(),
            cond: String::new(),
            run_when: String::new(),
            focused: 0,
            error: None,
        };
        let mut step = VisualStep::logic(VisualNodeKind::Xor, "gate");
        let err = editor.apply_to_step(&mut step).unwrap_err();
        assert!(err.contains("exactly 2"), "{}", err);
        editor.inputs = "a, b".into();
        assert!(editor.apply_to_step(&mut step).is_ok());
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
        assert_eq!(ENTITY_TYPE_IDS, ["cmd", "script", "app", "chain"]);
        assert_eq!(ENTITY_TYPE_IDS.len(), ENTITY_TYPE_NAMES.len());
        assert_eq!(COMMAND_FORM_FIELDS, 5);
        assert_eq!(PROJECT_FORM_FIELDS, 2);
        assert_eq!(WORKFLOW_FORM_FIELDS, 3);
        assert_eq!(SECRET_FORM_FIELDS, 8);
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

/// Lua truthiness helper emitted with every logic node (empty string and
/// nil/false count as false so gate ports are "typed" booleans).
const LUA_TRUTHY_HELPER: &str =
    "local function __t(v) return not (v == nil or v == false or v == '') end ";

/// Compile a Compare/IfElse operand: numbers and quoted strings become Lua
/// literals, everything else is treated as a Lua expression typed by the
/// user (e.g. `results["build"]` or `ctx.vars.limit`).
fn compile_operand(operand: &str) -> String {
    let trimmed = operand.trim();
    if trimmed.parse::<f64>().is_ok()
        || (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
    {
        trimmed.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Compile a logic node into its Lua script (US-FUT-07). Inputs are step
/// names of earlier steps; their results arrive via the engine's `results`
/// table. Returns Err for malformed nodes.
fn compile_logic_node(step: &VisualStep) -> Result<(String, Vec<String>), String> {
    let helper = LUA_TRUTHY_HELPER.to_string();
    match step.kind {
        VisualNodeKind::And | VisualNodeKind::Or | VisualNodeKind::Xor => {
            if step.inputs.is_empty() {
                return Err(format!(
                    "{} node '{}' needs inputs",
                    step.kind.label(),
                    step.name
                ));
            }
            if step.kind == VisualNodeKind::Xor && step.inputs.len() != 2 {
                return Err(format!("XOR node '{}' needs exactly 2 inputs", step.name));
            }
            let terms: Vec<String> = step
                .inputs
                .iter()
                .map(|i| format!("__t(results[\"{}\"])", i))
                .collect();
            let joiner = match step.kind {
                VisualNodeKind::And => " and ",
                VisualNodeKind::Or => " or ",
                _ => " ~= ",
            };
            Ok((
                format!("{}return {}", helper, terms.join(joiner)),
                step.inputs.clone(),
            ))
        }
        VisualNodeKind::Not => {
            if step.inputs.len() != 1 {
                return Err(format!("NOT node '{}' needs exactly 1 input", step.name));
            }
            Ok((
                format!("{}return not __t(results[\"{}\"])", helper, step.inputs[0]),
                step.inputs.clone(),
            ))
        }
        VisualNodeKind::Compare => Ok((
            format!(
                "return {} {} {}",
                compile_operand(&step.left),
                step.op.symbol(),
                compile_operand(&step.right)
            ),
            vec![],
        )),
        VisualNodeKind::IfElse => {
            if step.cond.trim().is_empty()
                || step.left.trim().is_empty()
                || step.right.trim().is_empty()
            {
                return Err(format!(
                    "If/Else node '{}' needs a condition and two branch step names",
                    step.name
                ));
            }
            Ok((
                format!(
                    "if {} then return results[\"{}\"] else return results[\"{}\"] end",
                    step.cond.trim(),
                    step.left.trim(),
                    step.right.trim()
                ),
                vec![step.left.trim().to_string(), step.right.trim().to_string()],
            ))
        }
        VisualNodeKind::Command => Ok((step.script.clone(), vec![])),
    }
}

/// Build a `WorkflowDefinition` from visual builder state (US-WF-03, US-WF-10,
/// US-FUT-07).
///
/// The definition serializes to JSON which is stored as the workflow entity
/// content; the Lua engine executes the steps in definition order, captures
/// each step's return into `results["<name>"]` and honors `run_when` gates.
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
    let mut compiled: Vec<WorkflowStep> = Vec::new();
    for (idx, s) in steps.iter().enumerate() {
        let (script, depends_on) = if s.kind == VisualNodeKind::Command {
            (s.script.clone(), vec![])
        } else {
            // Logic nodes may only reference EARLIER steps (linear DAG order)
            let earlier: Vec<&str> = steps[..idx].iter().map(|p| p.name.as_str()).collect();
            let (script, deps) = compile_logic_node(s)?;
            for dep in &deps {
                if !earlier.contains(&dep.as_str()) {
                    return Err(format!(
                        "Node '{}' references '{}', which is not an earlier step",
                        s.name, dep
                    ));
                }
            }
            (script, deps)
        };
        compiled.push(WorkflowStep {
            name: s.name.clone(),
            script,
            depends_on,
            run_when: s.run_when.clone(),
        });
    }
    Ok(WorkflowDefinition {
        name: name.to_string(),
        description: if description.trim().is_empty() {
            None
        } else {
            Some(description.trim().to_string())
        },
        steps: compiled,
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
