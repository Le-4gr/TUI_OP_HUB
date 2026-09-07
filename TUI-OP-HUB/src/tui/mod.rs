//! TUI layer.
//!
//! - [`modern_app`] / [`modern_ui`] — the live application (login, 9 tabs,
//!   forms, popups, dashboard monitor)
//! - [`list_state`] — reusable list/form state and entity types
//! - [`helpers`] — free helper functions (paths, formatting, terminal spawn)

pub mod helpers;
pub mod list_state;
pub mod modern_app;
pub mod modern_ui;
