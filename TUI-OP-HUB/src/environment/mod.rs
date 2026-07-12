//! Environment management (US-ENV-01..08).
//!
//! Provides environment variable management and project-specific environments.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::Entity;

/// Environment definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub description: Option<String>,
    pub variables: HashMap<String, String>,
    pub project_id: Option<String>,
    pub active: bool,
}

impl Environment {
    /// Parse environment from entity content.
    pub fn from_entity(entity: &Entity) -> AppResult<Self> {
        if entity.type_id != "env" {
            return Err(AppError::Validation(format!(
                "Entity {} is not an environment (type: {})",
                entity.id, entity.type_id
            )));
        }

        let content = entity
            .content
            .as_ref()
            .ok_or_else(|| AppError::Validation("Environment entity has no content".to_string()))?;

        // Try to parse as JSON first, then fall back to simple key=value format
        if let Ok(env) = serde_json::from_str::<Environment>(content) {
            Ok(env)
        } else {
            // Parse simple key=value format
            let mut variables = HashMap::new();
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    variables.insert(key.trim().to_string(), value.trim().to_string());
                }
            }

            Ok(Environment {
                name: entity.name.clone(),
                description: entity.description.clone(),
                variables,
                project_id: entity.project_id.clone(),
                active: false,
            })
        }
    }

    /// Convert environment to entity content.
    pub fn to_content(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Apply environment variables to current process.
    pub fn apply(&self) -> AppResult<()> {
        for (key, value) in &self.variables {
            std::env::set_var(key, value);
        }
        tracing::info!(env = %self.name, vars = self.variables.len(), "environment applied");
        Ok(())
    }

    /// Get environment variables as shell export commands.
    pub fn to_shell_exports(&self) -> String {
        self.variables
            .iter()
            .map(|(key, value)| format!("export {}=\"{}\"", key, value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Environment manager.
pub struct EnvironmentManager {
    current_env: Option<Environment>,
}

impl EnvironmentManager {
    pub fn new() -> Self {
        Self { current_env: None }
    }

    /// Switch to a new environment.
    pub fn switch_to(&mut self, env: Environment) -> AppResult<()> {
        env.apply()?;
        self.current_env = Some(env);
        Ok(())
    }

    /// Get current environment.
    pub fn current(&self) -> Option<&Environment> {
        self.current_env.as_ref()
    }

    /// Clear current environment.
    pub fn clear(&mut self) {
        self.current_env = None;
    }

    /// Create a project-specific environment.
    pub fn create_project_env(
        &self,
        project_id: String,
        name: String,
        variables: HashMap<String, String>,
    ) -> Environment {
        Environment {
            name,
            description: Some(format!("Environment for project {}", project_id)),
            variables,
            project_id: Some(project_id),
            active: false,
        }
    }
}

/// Python virtual environment management (US-ENV-03, US-ENV-04).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnv {
    pub name: String,
    pub path: PathBuf,
    pub python_version: Option<String>,
    pub active: bool,
}

impl PythonEnv {
    /// Create a new Python virtual environment.
    pub async fn create(name: String, python_path: Option<String>) -> AppResult<Self> {
        let venv_dir = std::env::current_dir()?.join(".venvs").join(&name);

        let python = python_path.unwrap_or_else(|| "python3".to_string());
        let output = tokio::process::Command::new(&python)
            .args(["-m", "venv"])
            .arg(&venv_dir)
            .output()
            .await?;

        if !output.status.success() {
            return Err(AppError::Other(format!(
                "Failed to create virtual environment: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Get Python version
        let python_version = Self::get_python_version(&venv_dir).await.ok();

        Ok(Self {
            name,
            path: venv_dir,
            python_version,
            active: false,
        })
    }

    /// Activate the virtual environment.
    pub fn activate(&self) -> AppResult<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        #[cfg(unix)]
        {
            let bin_path = self.path.join("bin");
            env_vars.insert(
                "VIRTUAL_ENV".to_string(),
                self.path.to_string_lossy().to_string(),
            );
            env_vars.insert(
                "PATH".to_string(),
                format!(
                    "{}:{}",
                    bin_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }

        #[cfg(windows)]
        {
            let scripts_path = self.path.join("Scripts");
            env_vars.insert(
                "VIRTUAL_ENV".to_string(),
                self.path.to_string_lossy().to_string(),
            );
            env_vars.insert(
                "PATH".to_string(),
                format!(
                    "{};{}",
                    scripts_path.to_string_lossy(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }

        Ok(env_vars)
    }

    async fn get_python_version(venv_path: &PathBuf) -> AppResult<String> {
        #[cfg(unix)]
        let python_exe = venv_path.join("bin").join("python");
        #[cfg(windows)]
        let python_exe = venv_path.join("Scripts").join("python.exe");

        let output = tokio::process::Command::new(&python_exe)
            .args(["--version"])
            .output()
            .await?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(AppError::Other("Failed to get Python version".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_parsing() {
        let content = r#"
# Development environment
DATABASE_URL=sqlite://dev.db
DEBUG=true
LOG_LEVEL=debug
        "#;

        let entity = Entity {
            id: "test".to_string(),
            name: "dev-env".to_string(),
            description: Some("Development environment".to_string()),
            content: Some(content.to_string()),
            type_id: "env".to_string(),
            project_id: None,
            metadata_json: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let env = Environment::from_entity(&entity).unwrap();
        assert_eq!(env.name, "dev-env");
        assert_eq!(env.variables.len(), 3);
        assert_eq!(
            env.variables.get("DATABASE_URL"),
            Some(&"sqlite://dev.db".to_string())
        );
        assert_eq!(env.variables.get("DEBUG"), Some(&"true".to_string()));
    }

    #[test]
    fn test_shell_exports() {
        let env = Environment {
            name: "test".to_string(),
            description: None,
            variables: {
                let mut vars = HashMap::new();
                vars.insert("FOO".to_string(), "bar".to_string());
                vars.insert("BAZ".to_string(), "qux".to_string());
                vars
            },
            project_id: None,
            active: false,
        };

        let exports = env.to_shell_exports();
        assert!(exports.contains("export FOO=\"bar\""));
        assert!(exports.contains("export BAZ=\"qux\""));
    }
}
