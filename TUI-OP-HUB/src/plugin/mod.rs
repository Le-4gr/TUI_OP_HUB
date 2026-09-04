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
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub plugin_type: String, // "rust", "lua", "python"
    pub entry_point: String,
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub optional_capabilities: Vec<String>,
    #[serde(default)]
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

/// Convert a serde_json value into a Lua table (nested values become their
/// string representations to keep the plugin boundary simple).
fn serde_json_to_lua_table(lua: &mlua::Lua, value: &serde_json::Value) -> AppResult<mlua::Value> {
    let table = lua.create_table()?;
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            let lv = match v {
                serde_json::Value::String(sv) => mlua::Value::String(lua.create_string(sv)?),
                serde_json::Value::Number(n) => {
                    mlua::Value::Integer(n.as_f64().unwrap_or(0.0) as i64)
                }
                serde_json::Value::Bool(bv) => mlua::Value::Boolean(*bv),
                other => mlua::Value::String(lua.create_string(other.to_string())?),
            };
            table.set(k.as_str(), lv)?;
        }
    }
    Ok(mlua::Value::Table(table))
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
        let lua = mlua::Lua::new();
        let script = std::fs::read_to_string(&self.script_path).map_err(|e| AppError::Io(e))?;
        install_sandbox(&lua, &self.capabilities)?;

        lua.load(&script)
            .exec()
            .map_err(|e| AppError::Other(format!("Lua load error: {}", e)))?;

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

/// Install a sandboxed host-function set into a plugin Lua state.
///
/// Host functions are gated by the approved capabilities (US-PLG-05/06):
/// `log` is always available; `run_command` requires `execute_commands`.
fn install_sandbox(lua: &mlua::Lua, capabilities: &[Capability]) -> AppResult<()> {
    let globals = lua.globals();

    // log(message): always available, side-effect free
    let log = lua.create_function(|_lua, message: String| {
        tracing::info!(plugin_log = %message);
        Ok(())
    })?;
    globals.set("log", log)?;

    // run_command(cmd): only with the execute_commands capability
    if capabilities.contains(&Capability::ExecuteCommands) {
        let run_command = lua.create_function(|_lua, command: String| {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .map_err(|e| mlua::Error::RuntimeError(format!("spawn failed: {}", e)))?;
            let result = _lua.create_table()?;
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
    }

    Ok(())
}

impl LuaPlugin {
    /// Call an event hook `on_<event>` in the plugin script, if defined.
    /// The payload is exposed to the script as the global `event` table.
    pub fn run_hook(&self, event: &str, payload: &serde_json::Value) -> AppResult<Option<String>> {
        let lua = mlua::Lua::new();
        let script = std::fs::read_to_string(&self.script_path).map_err(|e| AppError::Io(e))?;
        install_sandbox(&lua, &self.capabilities)?;
        lua.load(&script)
            .exec()
            .map_err(|e| AppError::Other(format!("Lua load error: {}", e)))?;
        let globals = lua.globals();
        let hook_name = format!("on_{}", event);
        let func: Option<mlua::Function> = globals.get(hook_name.as_str())?;
        let Some(func) = func else {
            return Ok(None); // plugin does not subscribe to this event
        };
        let event_table = serde_json_to_lua_table(&lua, payload)?;
        globals.set("event", event_table)?;
        let result: Option<String> = func
            .call(())
            .map_err(|e| AppError::Other(format!("hook error: {}", e)))?;
        Ok(result)
    }
}

/// Plugin manager
pub struct PluginManager {
    pool: Arc<SqlitePool>,
    plugins: Arc<RwLock<HashMap<String, LuaPlugin>>>,
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

        // Check if plugin is approved (US-PLG-06) and enabled (US-PLG-10)
        if !self.is_plugin_approved(&manifest.id).await? {
            return Err(AppError::Unauthorized(format!(
                "Plugin '{}' is not approved",
                manifest.id
            )));
        }
        if !self.is_plugin_enabled(&manifest.id).await? {
            return Err(AppError::Other(format!(
                "Plugin '{}' is disabled",
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
        self.plugins.write().await.insert(plugin_id, plugin);

        tracing::info!(plugin_id = %manifest.id, "Loaded Lua plugin");
        Ok(())
    }

    /// Check if a plugin is approved
    pub async fn is_plugin_approved(&self, plugin_id: &str) -> AppResult<bool> {
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
        // Ensure the plugin row exists so the FK on approvals holds even when
        // approving before first load (e.g. from the Plugins tab).
        sqlx::query(
            "INSERT OR IGNORE INTO plugins (id, name, version, plugin_type, capabilities, enabled) VALUES (?, ?, '0', 'lua', '[]', 1)",
        )
        .bind(plugin_id)
        .bind(plugin_id)
        .execute(&*self.pool)
        .await?;
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

impl PluginManager {
    /// The default plugin directory: `~/.config/tui-op-hub/plugins`.
    pub fn default_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/tui-op-hub/plugins")
    }

    /// Scan the plugin directory and return every discovered manifest,
    /// regardless of approval state (the TUI lists these, US-PLG-10).
    pub fn discover_plugins(&self) -> Vec<PluginManifest> {
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.plugin_dir) else {
            return found;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("plugin.toml");
            let Ok(text) = std::fs::read_to_string(&manifest_path) else {
                continue;
            };
            match toml::from_str::<PluginManifest>(&text) {
                Ok(m) => found.push(m),
                Err(e) => tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "invalid plugin.toml"
                ),
            }
        }
        found.sort_by(|x, y| x.id.cmp(&y.id));
        found
    }

    /// Dispatch an event to every loaded plugin that subscribes to it.
    pub async fn emit_event(
        &self,
        event: &str,
        payload: &serde_json::Value,
    ) -> Vec<(String, AppResult<Option<String>>)> {
        let plugins = self.plugins.read().await;
        let mut results = Vec::new();
        for (id, plugin) in plugins.iter() {
            results.push((id.clone(), plugin.run_hook(event, payload)));
        }
        results
    }

    /// Enable or disable a plugin in the database (US-PLG-10). Disabled
    /// plugins are unloaded; enabling re-loads from disk if approved.
    pub async fn set_plugin_enabled(&self, plugin_id: &str, enabled: bool) -> AppResult<()> {
        sqlx::query("UPDATE plugins SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(enabled as i32)
            .bind(plugin_id)
            .execute(&*self.pool)
            .await?;
        if !enabled {
            self.unload_plugin(plugin_id).await;
        } else {
            let dir = self.plugin_dir.join(plugin_id);
            if dir.is_dir() {
                let _ = self.load_plugin(&dir).await;
            }
        }
        Ok(())
    }

    /// Whether a plugin row exists and is enabled in the database.
    pub async fn is_plugin_enabled(&self, plugin_id: &str) -> AppResult<bool> {
        let row = sqlx::query("SELECT enabled FROM plugins WHERE id = ?")
            .bind(plugin_id)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(row
            .map(|r| r.try_get::<i32, _>("enabled").unwrap_or(0) != 0)
            .unwrap_or(false))
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

    /// In-memory pool with migrations for plugin tables.
    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    /// Write a git-automation-style plugin: on_project_created runs `git init`.
    fn write_plugin(dir: &Path, id: &str, hook_body: &str) -> PathBuf {
        let plugin_dir = dir.join(id);
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            format!(
                "id = '{}'\nname = '{}'\nversion = '1.0.0'\nplugin_type = 'lua'\nentry_point = 'main.lua'\nrequired_capabilities = ['execute_commands']\noptional_capabilities = []\n\n[commands]\nhello = 'Say hello'\n",
                id, id
            ),
        )
        .unwrap();
        std::fs::write(
            plugin_dir.join("main.lua"),
            format!(
                "function on_project_created()\n{}\nreturn 'hooked'\nend\n\nfunction hello(args)\nreturn 'hi ' .. tostring(args[1])\nend\n",
                hook_body
            ),
        )
        .unwrap();
        plugin_dir
    }

    #[tokio::test]
    async fn given_unapproved_plugin_when_loaded_then_unauthorized() {
        let pool = test_pool().await;
        let tmp = std::env::temp_dir().join(format!("plug-unappr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let manager = PluginManager::new(std::sync::Arc::new(pool), tmp.clone());
        write_plugin(
            &tmp,
            "unapproved.plugin",
            "local out = run_command('touch marker')",
        );

        let manifests = manager.discover_plugins();
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].id, "unapproved.plugin");

        let err = manager.load_plugin(&tmp.join("unapproved.plugin")).await;
        assert!(matches!(err, Err(AppError::Unauthorized(_))));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn given_approved_plugin_when_event_fired_then_hook_executes() {
        let pool = test_pool().await;
        let tmp = std::env::temp_dir().join(format!("plug-hook-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let manager = PluginManager::new(std::sync::Arc::new(pool), tmp.clone());
        let marker = tmp.join("hooked.txt");
        write_plugin(
            &tmp,
            "git.automation",
            &format!(
                "local out = run_command('echo hooked > {}')\nassert(out.success)",
                marker.display()
            ),
        );

        // Approve + register + load
        manager
            .approve_plugin("git.automation", "test-user")
            .await
            .unwrap();
        manager
            .load_plugin(&tmp.join("git.automation"))
            .await
            .unwrap();

        // Fire the event the way project creation does
        let payload = serde_json::json!({ "name": "myproj", "path": "/tmp/myproj" });
        let results = manager.emit_event("project_created", &payload).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "git.automation");
        assert_eq!(results[0].1.as_ref().unwrap().as_deref(), Some("hooked"));

        // The hook actually ran its command
        assert!(marker.exists(), "hook side effect executed");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn given_plugin_without_hook_when_event_fired_then_noop() {
        let pool = test_pool().await;
        let tmp = std::env::temp_dir().join(format!("plug-noop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let manager = PluginManager::new(std::sync::Arc::new(pool), tmp.clone());
        write_plugin(&tmp, "silent.plugin", "-- unreachable: no hook");
        // Overwrite main.lua with a script that defines NO hook function
        std::fs::write(
            tmp.join("silent.plugin/main.lua"),
            "function hello(args) return 'hi' end\n",
        )
        .unwrap();
        manager
            .approve_plugin("silent.plugin", "test-user")
            .await
            .unwrap();
        manager
            .load_plugin(&tmp.join("silent.plugin"))
            .await
            .unwrap();

        let results = manager
            .emit_event("project_created", &serde_json::json!({}))
            .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1.as_ref().unwrap().as_deref(), None);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn given_plugin_when_disabled_then_not_loaded_and_toggle_works() {
        let pool = test_pool().await;
        let tmp = std::env::temp_dir().join(format!("plug-dis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let manager = PluginManager::new(std::sync::Arc::new(pool), tmp.clone());
        write_plugin(&tmp, "toggle.plugin", "-- nothing");
        manager
            .approve_plugin("toggle.plugin", "test-user")
            .await
            .unwrap();
        manager
            .load_plugin(&tmp.join("toggle.plugin"))
            .await
            .unwrap();
        assert!(manager.is_plugin_enabled("toggle.plugin").await.unwrap());

        // Disable: unloads from memory + flips DB flag
        manager
            .set_plugin_enabled("toggle.plugin", false)
            .await
            .unwrap();
        assert!(!manager.is_plugin_enabled("toggle.plugin").await.unwrap());
        assert!(manager.list_plugins().await.is_empty());

        // Re-enable: reloads from disk
        manager
            .set_plugin_enabled("toggle.plugin", true)
            .await
            .unwrap();
        assert!(manager.is_plugin_enabled("toggle.plugin").await.unwrap());
        assert_eq!(manager.list_plugins().await.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
