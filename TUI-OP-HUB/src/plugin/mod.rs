use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Plugin capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadSecrets,
    WriteSecrets,
    ExecuteCommands,
    NetworkAccess,
    FileSystemRead,
    FileSystemWrite,
    DatabaseRead,
    DatabaseWrite,
}

impl Capability {
    pub fn as_str(&self) -> &str {
        match self {
            Capability::ReadSecrets => "read_secrets",
            Capability::WriteSecrets => "write_secrets",
            Capability::ExecuteCommands => "execute_commands",
            Capability::NetworkAccess => "network_access",
            Capability::FileSystemRead => "filesystem_read",
            Capability::FileSystemWrite => "filesystem_write",
            Capability::DatabaseRead => "database_read",
            Capability::DatabaseWrite => "database_write",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read_secrets" => Some(Capability::ReadSecrets),
            "write_secrets" => Some(Capability::WriteSecrets),
            "execute_commands" => Some(Capability::ExecuteCommands),
            "network_access" => Some(Capability::NetworkAccess),
            "filesystem_read" => Some(Capability::FileSystemRead),
            "filesystem_write" => Some(Capability::FileSystemWrite),
            "database_read" => Some(Capability::DatabaseRead),
            "database_write" => Some(Capability::DatabaseWrite),
            _ => None,
        }
    }
}

/// Plugin manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub plugin_type: String, // "rust", "lua", "python"
    pub entry_point: String,
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
    pub commands: HashMap<String, String>,
}

/// Plugin trait for all plugin types
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn capabilities(&self) -> &[Capability];
    fn execute(&self, command: &str, args: &[String]) -> AppResult<String>;
}

/// Lua plugin implementation
pub struct LuaPlugin {
    manifest: PluginManifest,
    capabilities: Vec<Capability>,
    script_path: PathBuf,
}

impl LuaPlugin {
    pub fn new(manifest: PluginManifest, script_path: PathBuf) -> AppResult<Self> {
        let capabilities = manifest
            .required_capabilities
            .iter()
            .chain(manifest.optional_capabilities.iter())
            .filter_map(|s| Capability::from_str(s))
            .collect();

        Ok(Self {
            manifest,
            capabilities,
            script_path,
        })
    }
}

impl Plugin for LuaPlugin {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn version(&self) -> &str {
        &self.manifest.version
    }

    fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    fn execute(&self, command: &str, args: &[String]) -> AppResult<String> {
        // Load and execute Lua script
        let lua = mlua::Lua::new();

        // Load the plugin script
        let script = std::fs::read_to_string(&self.script_path).map_err(|e| AppError::Io(e))?;

        lua.load(&script)
            .exec()
            .map_err(|e| AppError::Other(format!("Lua execution error: {}", e)))?;

        // Call the command function
        let globals = lua.globals();
        let func: mlua::Function = globals
            .get(command)
            .map_err(|e| AppError::Other(format!("Command not found: {}", e)))?;

        let result: String = func
            .call(args.to_vec())
            .map_err(|e| AppError::Other(format!("Command execution error: {}", e)))?;

        Ok(result)
    }
}

/// Plugin manager
pub struct PluginManager {
    pool: Arc<SqlitePool>,
    plugins: Arc<RwLock<HashMap<String, Box<dyn Plugin>>>>,
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(pool: Arc<SqlitePool>, plugin_dir: PathBuf) -> Self {
        Self {
            pool,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            plugin_dir,
        }
    }

    /// Load a plugin from a directory
    pub async fn load_plugin(&self, plugin_path: &Path) -> AppResult<()> {
        // Read plugin manifest
        let manifest_path = plugin_path.join("plugin.toml");
        let manifest_str = std::fs::read_to_string(&manifest_path).map_err(|e| AppError::Io(e))?;

        let manifest: PluginManifest = toml::from_str(&manifest_str)
            .map_err(|e| AppError::Other(format!("Invalid plugin manifest: {}", e)))?;

        // Check if plugin is approved
        if !self.is_plugin_approved(&manifest.id).await? {
            return Err(AppError::Unauthorized(format!(
                "Plugin '{}' is not approved",
                manifest.id
            )));
        }

        // Load based on plugin type
        match manifest.plugin_type.as_str() {
            "lua" => self.load_lua_plugin(plugin_path, manifest).await?,
            "rust" => return Err(AppError::Other("Rust plugins not yet implemented".into())),
            "python" => return Err(AppError::Other("Python plugins not yet implemented".into())),
            _ => {
                return Err(AppError::Validation(format!(
                    "Unknown plugin type: {}",
                    manifest.plugin_type
                )))
            }
        }

        Ok(())
    }

    /// Load a Lua plugin
    async fn load_lua_plugin(&self, plugin_path: &Path, manifest: PluginManifest) -> AppResult<()> {
        let script_path = plugin_path.join(&manifest.entry_point);

        if !script_path.exists() {
            return Err(AppError::Other(format!(
                "Plugin entry point not found: {}",
                script_path.display()
            )));
        }

        let plugin = LuaPlugin::new(manifest.clone(), script_path)?;
        let plugin_id = manifest.id.clone();

        // Register plugin in database
        self.register_plugin(&manifest).await?;

        // Add to loaded plugins
        self.plugins
            .write()
            .await
            .insert(plugin_id, Box::new(plugin));

        tracing::info!(plugin_id = %manifest.id, "Loaded Lua plugin");
        Ok(())
    }

    /// Check if a plugin is approved
    async fn is_plugin_approved(&self, plugin_id: &str) -> AppResult<bool> {
        let row = sqlx::query("SELECT approved FROM plugin_approvals WHERE plugin_id = ?")
            .bind(plugin_id)
            .fetch_optional(&*self.pool)
            .await?;

        Ok(row
            .map(|r| r.try_get::<i32, _>("approved").unwrap_or(0) != 0)
            .unwrap_or(false))
    }

    /// Register a plugin in the database
    async fn register_plugin(&self, manifest: &PluginManifest) -> AppResult<()> {
        let capabilities_json = serde_json::to_string(&manifest.required_capabilities)?;

        sqlx::query(
            r#"
            INSERT INTO plugins (id, name, version, plugin_type, capabilities, enabled)
            VALUES (?, ?, ?, ?, ?, 1)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                version = excluded.version,
                capabilities = excluded.capabilities
            "#,
        )
        .bind(&manifest.id)
        .bind(&manifest.name)
        .bind(&manifest.version)
        .bind(&manifest.plugin_type)
        .bind(&capabilities_json)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Approve a plugin
    pub async fn approve_plugin(&self, plugin_id: &str, user_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO plugin_approvals (plugin_id, user_id, approved)
            VALUES (?, ?, 1)
            ON CONFLICT(plugin_id, user_id) DO UPDATE SET approved = 1
            "#,
        )
        .bind(plugin_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;

        tracing::info!(plugin_id = %plugin_id, user_id = %user_id, "Approved plugin");
        Ok(())
    }

    /// Execute a plugin command
    pub async fn execute_command(
        &self,
        plugin_id: &str,
        command: &str,
        args: &[String],
    ) -> AppResult<String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or_else(|| AppError::Other(format!("Plugin not found: {}", plugin_id)))?;

        plugin.execute(command, args)
    }

    /// List all loaded plugins
    pub async fn list_plugins(&self) -> Vec<String> {
        self.plugins.read().await.keys().cloned().collect()
    }

    /// Unload a plugin
    pub async fn unload_plugin(&self, plugin_id: &str) -> AppResult<()> {
        self.plugins.write().await.remove(plugin_id);
        tracing::info!(plugin_id = %plugin_id, "Unloaded plugin");
        Ok(())
    }

    /// Scan plugin directory and load all approved plugins
    pub async fn load_all_plugins(&self) -> AppResult<()> {
        if !self.plugin_dir.exists() {
            std::fs::create_dir_all(&self.plugin_dir).map_err(|e| AppError::Io(e))?;
            return Ok(());
        }

        let entries = std::fs::read_dir(&self.plugin_dir).map_err(|e| AppError::Io(e))?;

        for entry in entries {
            let entry = entry.map_err(|e| AppError::Io(e))?;
            let path = entry.path();

            if path.is_dir() {
                if let Err(e) = self.load_plugin(&path).await {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to load plugin"
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_conversion() {
        assert_eq!(Capability::ReadSecrets.as_str(), "read_secrets");
        assert_eq!(
            Capability::from_str("execute_commands"),
            Some(Capability::ExecuteCommands)
        );
        assert_eq!(Capability::from_str("invalid"), None);
    }

    #[test]
    fn test_manifest_parsing() {
        let toml_str = r#"
            id = "com.example.test"
            name = "Test Plugin"
            version = "1.0.0"
            plugin_type = "lua"
            entry_point = "main.lua"
            required_capabilities = ["execute_commands"]
            optional_capabilities = ["network_access"]

            [commands]
            hello = "Say hello"
        "#;

        let manifest: PluginManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.id, "com.example.test");
        assert_eq!(manifest.plugin_type, "lua");
        assert_eq!(manifest.required_capabilities.len(), 1);
    }
}
