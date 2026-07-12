//! Config File Management (US-CFG-01..08).
//!
//! Provides centralized config file storage, symlinking, and versioning.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Config file entry (US-CFG-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub target_path: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub project_id: Option<String>,
    pub content: Option<String>,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// Config version snapshot (US-CFG-03).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub version: u32,
    pub content: String,
    pub created_at: String,
    pub checksum: String,
}

/// Config manager.
pub struct ConfigManager {
    storage_dir: PathBuf,
}

impl ConfigManager {
    pub fn new(storage_dir: impl Into<PathBuf>) -> Self {
        Self {
            storage_dir: storage_dir.into(),
        }
    }

    /// Store a config file centrally (US-CFG-01).
    pub fn store_config(&self, entry: &ConfigEntry) -> AppResult<()> {
        let config_dir = self.storage_dir.join(&entry.id);
        fs::create_dir_all(&config_dir)?;

        let content_path = config_dir.join(format!("v{}.conf", entry.version));
        if let Some(content) = &entry.content {
            fs::write(&content_path, content)?;
        } else if Path::new(&entry.source_path).exists() {
            fs::copy(&entry.source_path, &content_path)?;
        }

        let metadata_path = config_dir.join("metadata.json");
        let metadata = serde_json::to_string_pretty(entry)?;
        fs::write(&metadata_path, metadata)?;

        tracing::info!(id = %entry.id, version = entry.version, "config stored");
        Ok(())
    }

    /// Symlink config to target location (US-CFG-02).
    pub fn symlink_config(&self, entry: &ConfigEntry) -> AppResult<()> {
        let config_dir = self.storage_dir.join(&entry.id);
        let content_path = config_dir.join(format!("v{}.conf", entry.version));

        if !content_path.exists() {
            return Err(AppError::NotFound {
                entity: "config",
                id: entry.id.clone(),
            });
        }

        if Path::new(&entry.target_path).exists() {
            fs::remove_file(&entry.target_path)?;
        }

        if let Some(parent) = Path::new(&entry.target_path).parent() {
            fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(&content_path, &entry.target_path)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&content_path, &entry.target_path)?;

        tracing::info!(id = %entry.id, target = %entry.target_path, "config symlinked");
        Ok(())
    }

    /// Track config versions (US-CFG-03).
    pub fn save_version(&self, entry: &ConfigEntry, content: &str) -> AppResult<ConfigVersion> {
        let version = entry.version + 1;
        let checksum = Self::checksum(content);
        let timestamp = chrono::Utc::now().to_rfc3339();

        let version_entry = ConfigVersion {
            version,
            content: content.to_string(),
            created_at: timestamp,
            checksum,
        };

        let config_dir = self.storage_dir.join(&entry.id);
        fs::create_dir_all(&config_dir)?;

        let version_path = config_dir.join(format!("v{}.conf", version));
        fs::write(&version_path, content)?;

        let version_meta_path = config_dir.join(format!("v{}.meta.json", version));
        let meta = serde_json::to_string_pretty(&version_entry)?;
        fs::write(&version_meta_path, meta)?;

        tracing::info!(id = %entry.id, version, "config version saved");
        Ok(version_entry)
    }

    /// List config versions (US-CFG-03).
    pub fn list_versions(&self, config_id: &str) -> AppResult<Vec<ConfigVersion>> {
        let config_dir = self.storage_dir.join(config_id);
        if !config_dir.exists() {
            return Ok(Vec::new());
        }

        let mut versions = Vec::new();
        for entry in fs::read_dir(&config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".meta.json") {
                    let content = fs::read_to_string(&path)?;
                    if let Ok(version) = serde_json::from_str::<ConfigVersion>(&content) {
                        versions.push(version);
                    }
                }
            }
        }

        versions.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(versions)
    }

    /// Edit a config file inline (US-CFG-07).
    pub fn edit_config(&self, config_id: &str, version: u32, new_content: &str) -> AppResult<()> {
        let config_dir = self.storage_dir.join(config_id);
        let content_path = config_dir.join(format!("v{}.conf", version));
        fs::write(&content_path, new_content)?;
        tracing::info!(id = config_id, version, "config edited");
        Ok(())
    }

    /// Read config content.
    pub fn read_config(&self, config_id: &str, version: u32) -> AppResult<String> {
        let config_dir = self.storage_dir.join(config_id);
        let content_path = config_dir.join(format!("v{}.conf", version));
        fs::read_to_string(&content_path).map_err(AppError::Io)
    }

    /// Group configs by project (US-CFG-05).
    pub fn group_by_project<'a>(
        &self,
        entries: &'a [ConfigEntry],
    ) -> HashMap<String, Vec<&'a ConfigEntry>> {
        let mut groups: HashMap<String, Vec<&'a ConfigEntry>> = HashMap::new();
        for entry in entries {
            let key = entry.project_id.clone().unwrap_or_else(|| "ungrouped".to_string());
            groups.entry(key).or_default().push(entry);
        }
        groups
    }

    /// Filter configs by tags (US-CFG-04).
    pub fn filter_by_tags<'a>(
        &self,
        entries: &'a [ConfigEntry],
        tags: &[String],
    ) -> Vec<&'a ConfigEntry> {
        entries
            .iter()
            .filter(|e| tags.iter().any(|t| e.tags.contains(t)))
            .collect()
    }

    fn checksum(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum() {
        let c1 = ConfigManager::checksum("hello");
        let c2 = ConfigManager::checksum("hello");
        let c3 = ConfigManager::checksum("world");
        assert_eq!(c1, c2);
        assert_ne!(c1, c3);
    }

    #[test]
    fn test_filter_by_tags() {
        let entries = vec![
            ConfigEntry {
                id: "1".to_string(),
                name: "shell".to_string(),
                source_path: "/tmp/a".to_string(),
                target_path: "/tmp/b".to_string(),
                description: None,
                tags: vec!["shell".to_string(), "bash".to_string()],
                project_id: None,
                content: None,
                version: 1,
                created_at: "".to_string(),
                updated_at: "".to_string(),
            },
            ConfigEntry {
                id: "2".to_string(),
                name: "firewall".to_string(),
                source_path: "/tmp/c".to_string(),
                target_path: "/tmp/d".to_string(),
                description: None,
                tags: vec!["network".to_string()],
                project_id: None,
                content: None,
                version: 1,
                created_at: "".to_string(),
                updated_at: "".to_string(),
            },
        ];
        let cm = ConfigManager::new("/tmp/test_configs");
        let filtered = cm.filter_by_tags(&entries, &["shell".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "shell");
    }
}