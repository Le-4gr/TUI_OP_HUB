//! Data models (US-CMD-01..09, US-PROJ-01..06, US-SRCH-01..04).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub type_id: String,
    pub project_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Parent command family (options are children of a `cmd`; US-CMD-01).
    #[sqlx(default)]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Environment kind for this project: `venv`, `pyenv`, `conda`, … (US-ENV-01)
    #[sqlx(default)]
    pub env_type: Option<String>,
    /// Shell command that activates the project environment, e.g.
    /// `source .venv/bin/activate` (US-ENV-01).
    #[sqlx(default)]
    pub env_cmd: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateEntity {
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub type_id: String,
    pub project_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub type_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntityType {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkflowRun {
    pub run_id: String,
    pub workflow_id: String,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub steps_completed: Option<i32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// True for admin users (the first registered user is admin; US-SEC).
    #[sqlx(default)]
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserKey {
    pub id: String,
    pub user_id: String,
    pub key_b64: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Secret {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub value_enc: String,
    pub created_at: String,
    pub updated_at: String,
    /// Classification: `password`, `ssh_key`, `gpg_key`, `api_key`, … (US-SEC-01)
    #[sqlx(default)]
    pub secret_kind: String,
    /// When true, using this secret requires re-entering the login password
    /// (key material etc. — passwords alone are not enough; US-SEC-05).
    #[sqlx(default)]
    pub requires_reauth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PluginApproval {
    pub id: String,
    pub plugin_id: String,
    pub action: String,
    pub status: String,
    pub requester: Option<String>,
    pub approver: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SshHost {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: i32,
    pub username: Option<String>,
    pub key_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduledTask {
    pub id: String,
    pub workflow_id: String,
    pub cron_expr: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// Display implementations for TUI rendering
impl std::fmt::Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}",
            self.name,
            self.description.as_deref().unwrap_or("No description")
        )
    }
}

impl std::fmt::Display for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}",
            self.name,
            self.description.as_deref().unwrap_or("No description")
        )
    }
}

impl std::fmt::Display for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.user_id)
    }
}
