//! Config File Management (US-CFG-01..08).
//!
//! Provides centralized config file storage, symlinking, and versioning.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Config file entry (US-CFG-01, US-CFG-09).
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
    /// Deployment targets (US-CFG-10): one config can serve many consumers.
    #[serde(default)]
    pub targets: Vec<String>,
    /// How the config reaches its targets (US-CFG-10).
    #[serde(default)]
    pub deploy_mode: DeployMode,
}

impl std::fmt::Display for ConfigEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let targets = if self.targets.is_empty() {
            "no targets".to_string()
        } else {
            format!("{} target(s)", self.targets.len())
        };
        write!(
            f,
            "{} - v{} [{}] ({})",
            self.name,
            self.version,
            self.deploy_mode.label(),
            targets
        )
    }
}

/// How a managed config reaches its targets (US-CFG-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DeployMode {
    #[default]
    Symlink,
    HardLink,
    Copy,
}

impl DeployMode {
    pub fn label(self) -> &'static str {
        match self {
            DeployMode::Symlink => "symlink",
            DeployMode::HardLink => "hard link",
            DeployMode::Copy => "copy",
        }
    }

    /// Cycle order for the `m` key: Symlink → HardLink → Copy → Symlink.
    pub fn next(self) -> Self {
        match self {
            DeployMode::Symlink => DeployMode::HardLink,
            DeployMode::HardLink => DeployMode::Copy,
            DeployMode::Copy => DeployMode::Symlink,
        }
    }
}

/// Result of deploying one target (US-CFG-10/11).
#[derive(Debug, Clone)]
pub struct Deployment {
    pub target: String,
    pub mode: DeployMode,
    pub ok: bool,
    /// True when the deployed copy DIFFERED from the master before the
    /// deployment (copy mode) or the link was missing/broken (link modes).
    pub had_drift: bool,
    pub message: String,
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
            let key = entry
                .project_id
                .clone()
                .unwrap_or_else(|| "ungrouped".to_string());
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

    // ------------------------------------------------------------------
    // Registry persistence (US-CFG-09): managed configs survive restarts
    // ------------------------------------------------------------------

    fn registry_path(&self) -> PathBuf {
        self.storage_dir.join("registry.json")
    }

    /// Load the managed-config registry; missing or corrupt file → empty.
    pub fn load_registry(&self) -> Vec<ConfigEntry> {
        let path = self.registry_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Persist the managed-config registry.
    pub fn save_registry(&self, entries: &[ConfigEntry]) -> AppResult<()> {
        fs::create_dir_all(&self.storage_dir)?;
        let json = serde_json::to_string_pretty(entries)?;
        fs::write(self.registry_path(), json)?;
        Ok(())
    }

    /// Path of the master copy for an entry.
    pub fn master_path(&self, entry: &ConfigEntry) -> PathBuf {
        self.storage_dir
            .join(&entry.id)
            .join(format!("v{}.conf", entry.version))
    }

    /// Register an EXISTING file (or folder tree, copied recursively) as a
    /// managed config: the master copy is stored centrally (US-CFG-09).
    pub fn register_existing(&self, source: &Path, name: &str) -> AppResult<ConfigEntry> {
        self.register_existing_full(
            source,
            name,
            None,
            Vec::new(),
            Vec::new(),
            DeployMode::default(),
        )
    }

    /// Register an existing file with full metadata (US-CFG-09): description,
    /// tags, deploy targets and deploy mode come from the TUI register form.
    /// Targets may point anywhere (and need not exist yet) — the source file
    /// stays where it is; deploys materialize it at each target.
    pub fn register_existing_full(
        &self,
        source: &Path,
        name: &str,
        description: Option<String>,
        tags: Vec<String>,
        targets: Vec<String>,
        deploy_mode: DeployMode,
    ) -> AppResult<ConfigEntry> {
        if !source.exists() {
            return Err(AppError::NotFound {
                entity: "config source",
                id: source.to_string_lossy().to_string(),
            });
        }
        let id = uuid::Uuid::new_v4().to_string();
        let entry = ConfigEntry {
            id: id.clone(),
            name: name.to_string(),
            source_path: source.to_string_lossy().to_string(),
            target_path: String::new(),
            description,
            tags,
            project_id: None,
            content: None,
            version: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            targets,
            deploy_mode,
        };
        self.store_config(&entry)?;
        let mut registry = self.load_registry();
        registry.push(entry.clone());
        self.save_registry(&registry)?;
        Ok(entry)
    }

    /// Remove a managed config from the registry and delete its master copy.
    pub fn remove_entry(&self, id: &str) -> AppResult<()> {
        let mut registry = self.load_registry();
        registry.retain(|e| e.id != id);
        self.save_registry(&registry)?;
        let dir = self.storage_dir.join(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Copy the SOURCE onto the master when they differ; bumps the version
    /// and returns `true` when the master was updated (update action, US-CFG-11).
    pub fn sync_source(&self, entry: &mut ConfigEntry) -> AppResult<bool> {
        let source = PathBuf::from(&entry.source_path);
        if !source.exists() {
            return Ok(false);
        }
        let master = self.master_path(entry);
        let source_content = fs::read_to_string(&source).unwrap_or_default();
        let master_content = fs::read_to_string(&master).unwrap_or_default();
        if source_content == master_content {
            return Ok(false);
        }
        let version = self.save_version(entry, &source_content)?;
        entry.version = version.version;
        self.store_config(entry)?;
        Ok(true)
    }

    /// Deploy a config to ALL of its targets using the entry's mode
    /// (US-CFG-10). Each result reports whether drift was seen (US-CFG-11).
    pub fn deploy(&self, entry: &ConfigEntry) -> AppResult<Vec<Deployment>> {
        let master = self.master_path(entry);
        if !master.exists() {
            return Err(AppError::NotFound {
                entity: "config",
                id: entry.id.clone(),
            });
        }
        Ok(entry
            .targets
            .iter()
            .map(|target| self.deploy_one(entry, &master, target))
            .collect())
    }

    /// True when `link` exists and resolves to `master` (symlink target or
    /// same inode for hard links).
    fn link_points_to(link: &Path, master: &Path) -> bool {
        let link_meta = match fs::symlink_metadata(link) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if link_meta.file_type().is_symlink() {
            return fs::read_link(link).map(|t| t == master).unwrap_or(false);
        }
        // Hard link: same device + inode as the master
        match (fs::metadata(link), fs::metadata(master)) {
            (Ok(l), Ok(m)) => {
                use std::os::unix::fs::MetadataExt;
                l.dev() == m.dev() && l.ino() == m.ino()
            }
            _ => false,
        }
    }

    /// Deploy one target (US-CFG-10): symlink, hard link, or copy — with
    /// drift detection (US-CFG-11).
    fn deploy_one(&self, entry: &ConfigEntry, master: &Path, target: &str) -> Deployment {
        let mut d = Deployment {
            target: target.to_string(),
            mode: entry.deploy_mode,
            ok: false,
            had_drift: false,
            message: String::new(),
        };
        let target_path = Path::new(target);
        match entry.deploy_mode {
            DeployMode::Copy => {
                // Drift: the deployed copy differed from the master
                if let (Ok(deployed), Ok(master_content)) =
                    (fs::read_to_string(target_path), fs::read_to_string(master))
                {
                    d.had_drift = deployed != master_content;
                }
                if let Some(parent) = target_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                match fs::copy(master, target_path) {
                    Ok(_) => {
                        d.ok = true;
                        d.message = "copied".to_string();
                    }
                    Err(e) => d.message = format!("copy failed: {}", e),
                }
            }
            DeployMode::Symlink | DeployMode::HardLink => {
                // Drift: a link EXISTS but is broken or points somewhere else
                // (a missing target is a fresh deploy, not drift)
                d.had_drift = target_path.symlink_metadata().is_ok()
                    && !Self::link_points_to(target_path, master);
                if target_path.symlink_metadata().is_ok() {
                    let _ = fs::remove_file(target_path);
                }
                if let Some(parent) = target_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let result = if entry.deploy_mode == DeployMode::Symlink {
                    #[cfg(unix)]
                    {
                        std::os::unix::fs::symlink(master, target_path)
                    }
                    #[cfg(windows)]
                    {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Unsupported,
                            "symlink unsupported",
                        ))
                    }
                } else {
                    fs::hard_link(master, target_path)
                };
                match result {
                    Ok(_) => {
                        d.ok = true;
                        d.message = if entry.deploy_mode == DeployMode::Symlink {
                            "symlinked".to_string()
                        } else {
                            "hard linked".to_string()
                        };
                    }
                    Err(e) => d.message = format!("deploy failed: {}", e),
                }
            }
        }
        d
    }

    // ------------------------------------------------------------------
    // Per-manager git integration (US-CFG-12): the storage dir is one repo
    // holding every managed config.
    // ------------------------------------------------------------------

    fn run_git(&self, args: &[&str]) -> AppResult<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&self.storage_dir)
            .output()
            .map_err(|e| AppError::Other(format!("git not available: {}", e)))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            return Err(AppError::Other(format!("git {}: {}", args[0], text.trim())));
        }
        Ok(text.trim().to_string())
    }

    /// `git init` in the storage dir (idempotent). Returns the output.
    pub fn git_init(&self) -> AppResult<String> {
        fs::create_dir_all(&self.storage_dir)?;
        if self.storage_dir.join(".git").exists() {
            return Ok("already a git repository".to_string());
        }
        self.run_git(&["init", "-q"])
    }

    /// Stage everything and commit; returns the commit summary.
    pub fn git_commit(&self, message: &str) -> AppResult<String> {
        self.git_init()?;
        self.run_git(&["add", "-A"])?;
        // Nothing to commit is not an error for the caller
        let status = self.run_git(&["status", "--porcelain"])?;
        if status.is_empty() {
            return Ok("nothing to commit".to_string());
        }
        self.run_git(&["commit", "-q", "-m", message])?;
        self.run_git(&["log", "-1", "--oneline"])
    }

    /// Recent commit history (`git log --oneline`).
    pub fn git_log(&self) -> AppResult<String> {
        self.run_git(&["log", "--oneline", "-10"])
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
                targets: Vec::new(),
                deploy_mode: DeployMode::default(),
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
                targets: Vec::new(),
                deploy_mode: DeployMode::default(),
            },
        ];
        let cm = ConfigManager::new("/tmp/test_configs");
        let filtered = cm.filter_by_tags(&entries, &["shell".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "shell");
    }
}

#[cfg(test)]
mod deploy_tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tuihub-cfg-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn register(man: &ConfigManager, src: &Path, name: &str) -> ConfigEntry {
        man.register_existing(src, name).unwrap()
    }

    #[test]
    fn given_existing_file_when_registered_then_master_copy_stored() {
        let dir = temp_dir("reg");
        let src = dir.join("hyprland.conf");
        fs::write(&src, "monitor=eDP-1,1920x1080").unwrap();
        let man = ConfigManager::new(dir.join("store"));
        let entry = register(&man, &src, "hyprland");
        let master = man.master_path(&entry);
        assert!(master.exists());
        assert_eq!(
            fs::read_to_string(master).unwrap(),
            "monitor=eDP-1,1920x1080"
        );
        // Registry round trip (US-CFG-09)
        let reg = man.load_registry();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg[0].name, "hyprland");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn given_missing_source_when_registered_then_not_found() {
        let dir = temp_dir("miss");
        let man = ConfigManager::new(dir.join("store"));
        assert!(man.register_existing(&dir.join("nope"), "x").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn given_copy_mode_when_deployed_then_drift_detected_and_fixed() {
        let dir = temp_dir("copy");
        let src = dir.join("sway.conf");
        fs::write(&src, "v1").unwrap();
        let man = ConfigManager::new(dir.join("store"));
        let mut entry = register(&man, &src, "sway");
        entry.deploy_mode = DeployMode::Copy;
        entry.targets = vec![dir.join("deployed").to_string_lossy().to_string()];
        man.save_registry(&[entry.clone()]).unwrap();

        // Deploy: no drift on first run (target did not exist)
        let r = man.deploy(&entry).unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].ok);
        assert!(!r[0].had_drift);

        // Change the SOURCE: the deployed copy is now STALE = drift (US-CFG-11)
        fs::write(&src, "v2").unwrap();
        man.sync_source(&mut entry).unwrap();
        let r = man.deploy(&entry).unwrap();
        assert!(r[0].had_drift, "stale deployed copy counts as drift");

        // Tamper with the DEPLOYED copy, deploy: drift detected (US-CFG-11)
        fs::write(&entry.targets[0], "tampered").unwrap();
        let r = man.deploy(&entry).unwrap();
        assert!(r[0].had_drift);
        assert_eq!(fs::read_to_string(&entry.targets[0]).unwrap(), "v2");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn given_symlink_mode_when_deployed_then_link_points_to_master() {
        let dir = temp_dir("link");
        let src = dir.join("kitty.conf");
        fs::write(&src, "font_size 12").unwrap();
        let man = ConfigManager::new(dir.join("store"));
        let mut entry = register(&man, &src, "kitty");
        entry.deploy_mode = DeployMode::Symlink;
        entry.targets = vec![dir.join("linked").to_string_lossy().to_string()];

        let r = man.deploy(&entry).unwrap();
        assert!(r[0].ok);
        assert!(!r[0].had_drift, "fresh deploy is not drift");
        let link = std::fs::symlink_metadata(&entry.targets[0]).unwrap();
        assert!(link.file_type().is_symlink());

        // Master gone: deploy must fail cleanly (NotFound), not panic
        fs::remove_file(man.master_path(&entry)).unwrap();
        assert!(man.deploy(&entry).is_err(), "master gone -> error");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn given_hard_link_mode_when_deployed_then_same_inode() {
        let dir = temp_dir("hard");
        let src = dir.join("gitconfig");
        fs::write(&src, "[user]").unwrap();
        let man = ConfigManager::new(dir.join("store"));
        let mut entry = register(&man, &src, "git");
        entry.deploy_mode = DeployMode::HardLink;
        entry.targets = vec![dir.join("target").to_string_lossy().to_string()];

        man.deploy(&entry).unwrap();
        use std::os::unix::fs::MetadataExt;
        let a = fs::metadata(man.master_path(&entry)).unwrap();
        let b = fs::metadata(&entry.targets[0]).unwrap();
        assert_eq!(a.ino(), b.ino(), "hard link shares the inode");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn given_source_changed_when_synced_then_version_bumps() {
        let dir = temp_dir("sync");
        let src = dir.join("zshrc");
        fs::write(&src, "export A=1").unwrap();
        let man = ConfigManager::new(dir.join("store"));
        let mut entry = register(&man, &src, "zsh");
        assert_eq!(entry.version, 1);

        // No change -> no bump
        assert!(!man.sync_source(&mut entry).unwrap());
        assert_eq!(entry.version, 1);

        // Change -> version 2 (US-CFG-11 update action)
        fs::write(&src, "export A=2").unwrap();
        assert!(man.sync_source(&mut entry).unwrap());
        assert_eq!(entry.version, 2);
        assert!(man.master_path(&entry).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deploy_mode_cycles() {
        assert_eq!(DeployMode::Symlink.next(), DeployMode::HardLink);
        assert_eq!(DeployMode::HardLink.next(), DeployMode::Copy);
        assert_eq!(DeployMode::Copy.next(), DeployMode::Symlink);
    }

    #[test]
    fn given_git_available_when_committed_then_log_shows_commit() {
        if crate::keygen::which("git") == false {
            return; // git not installed: skip gracefully
        }
        let dir = temp_dir("git");
        let man = ConfigManager::new(dir.join("store"));
        let src = dir.join("x.conf");
        fs::write(&src, "x").unwrap();
        register(&man, &src, "x");

        man.git_init().unwrap();
        let out = man.git_commit("first config").unwrap();
        assert!(!out.is_empty());
        let log = man.git_log().unwrap();
        assert!(log.contains("first config"), "log shows commit: {}", log);
        // Idempotent init
        assert!(man.git_init().unwrap().contains("already"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn given_register_existing_full_when_called_then_metadata_is_persisted() {
        let dir = temp_dir("full");
        let man = ConfigManager::new(dir.join("store"));
        let src = dir.join("app.conf");
        fs::write(&src, "key=value").unwrap();

        let entry = man
            .register_existing_full(
                &src,
                "app.conf",
                Some("main app config".to_string()),
                vec!["app".to_string(), "gui".to_string()],
                vec!["/tmp/where/it/should/be.conf".to_string()],
                DeployMode::Copy,
            )
            .unwrap();

        assert_eq!(entry.name, "app.conf");
        assert_eq!(entry.description.as_deref(), Some("main app config"));
        assert_eq!(entry.tags, vec!["app".to_string(), "gui".to_string()]);
        assert_eq!(entry.targets, vec!["/tmp/where/it/should/be.conf"]);
        assert_eq!(entry.deploy_mode, DeployMode::Copy);
        // Round-trips through the registry
        let stored = man.load_registry();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].description.as_deref(), Some("main app config"));
        assert_eq!(stored[0].targets, vec!["/tmp/where/it/should/be.conf"]);
        assert_eq!(stored[0].deploy_mode, DeployMode::Copy);
        // Legacy register_existing delegates with defaults
        let src2 = dir.join("b.conf");
        fs::write(&src2, "b").unwrap();
        let plain = man.register_existing(&src2, "b.conf").unwrap();
        assert_eq!(plain.description, None);
        assert!(plain.tags.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
