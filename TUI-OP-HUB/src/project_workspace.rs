//! Project workspace creation (US-PROJ, US-ENV).
//!
//! Creates project directories with a chosen environment, git-initializes
//! them, and manages tool integrations (editors, TUI apps, containers).

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// The environment type for a new project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Python,
    Rust,
    Node,
    Docker,
    Kubernetes,
    Generic,
}

impl ProjectKind {
    pub fn all() -> &'static [ProjectKind] {
        &[
            ProjectKind::Python,
            ProjectKind::Rust,
            ProjectKind::Node,
            ProjectKind::Docker,
            ProjectKind::Kubernetes,
            ProjectKind::Generic,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ProjectKind::Python => "python",
            ProjectKind::Rust => "rust",
            ProjectKind::Node => "node",
            ProjectKind::Docker => "docker",
            ProjectKind::Kubernetes => "kubernetes",
            ProjectKind::Generic => "generic",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ProjectKind::Python => "Python project with venv",
            ProjectKind::Rust => "Rust project with cargo",
            ProjectKind::Node => "Node.js project with npm",
            ProjectKind::Docker => "Docker Compose project",
            ProjectKind::Kubernetes => "Kubernetes manifests",
            ProjectKind::Generic => "Generic directory",
        }
    }

    pub fn from_name(name: &str) -> Option<ProjectKind> {
        ProjectKind::all()
            .iter()
            .find(|k| k.name() == name)
            .copied()
    }
}

/// The editor/TUI app to open a project with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectEditor {
    Neovim,
    Vim,
    VsCode,
    Lazygit,
    Lazydocker,
    Yazi,
    Helix,
}

impl ProjectEditor {
    pub fn all() -> &'static [ProjectEditor] {
        &[
            ProjectEditor::Neovim,
            ProjectEditor::Vim,
            ProjectEditor::VsCode,
            ProjectEditor::Lazygit,
            ProjectEditor::Lazydocker,
            ProjectEditor::Yazi,
            ProjectEditor::Helix,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ProjectEditor::Neovim => "nvim",
            ProjectEditor::Vim => "vim",
            ProjectEditor::VsCode => "code",
            ProjectEditor::Lazygit => "lazygit",
            ProjectEditor::Lazydocker => "lazydocker",
            ProjectEditor::Yazi => "yazi",
            ProjectEditor::Helix => "hx",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ProjectEditor::Neovim => "Neovim",
            ProjectEditor::Vim => "Vim",
            ProjectEditor::VsCode => "VS Code",
            ProjectEditor::Lazygit => "lazygit",
            ProjectEditor::Lazydocker => "lazydocker",
            ProjectEditor::Yazi => "yazi",
            ProjectEditor::Helix => "Helix",
        }
    }

    /// The command to open a directory with this editor.
    pub fn open_command(&self, dir: &str) -> String {
        match self {
            ProjectEditor::Neovim => format!("nvim {}", dir),
            ProjectEditor::Vim => format!("vim {}", dir),
            ProjectEditor::VsCode => format!("code {}", dir),
            ProjectEditor::Lazygit => format!("lazygit -p {}", dir),
            ProjectEditor::Lazydocker => format!("lazydocker -p {}", dir),
            ProjectEditor::Yazi => format!("yazi {}", dir),
            ProjectEditor::Helix => format!("hx {}", dir),
        }
    }

    pub fn from_name(name: &str) -> Option<ProjectEditor> {
        ProjectEditor::all()
            .iter()
            .find(|e| e.name() == name)
            .copied()
    }
}

/// What was created for a project.
#[derive(Debug, Clone)]
pub struct CreatedProject {
    pub path: PathBuf,
    pub git_initialized: bool,
    pub env_cmd: Option<String>,
}

/// Create a project directory with the chosen environment + git repo.
pub fn create_project_directory(
    parent_dir: &Path,
    name: &str,
    kind: &ProjectKind,
    overwrite: bool,
) -> AppResult<CreatedProject> {
    let dir = parent_dir.join(name);
    if dir.exists() && !overwrite {
        return Err(AppError::Other(format!(
            "directory '{}' already exists",
            dir.display()
        )));
    }
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Other(format!("mkdir: {e}")))?;

    // git init + branch rename to `main`
    let git_available = crate::keygen::which("git");
    let git_initialized = git_available && run_quiet(&dir, "git", &["init"]);
    if git_initialized {
        run_quiet(&dir, "git", &["branch", "-m", "main"]);
        let gitignore = match kind {
            ProjectKind::Python => "venv/\n__pycache__/\n*.pyc\n.env\n",
            ProjectKind::Rust => "target/\n",
            ProjectKind::Node => "node_modules/\ndist/\n.env\n",
            ProjectKind::Docker | ProjectKind::Kubernetes => ".env\n*.log\n",
            ProjectKind::Generic => "",
        };
        if !gitignore.is_empty() {
            let _ = std::fs::write(dir.join(".gitignore"), gitignore);
        }
    }

    let env_cmd = match kind {
        ProjectKind::Python => {
            let _ = run_quiet(&dir, "python3", &["-m", "venv", "venv"]);
            Some("source venv/bin/activate".to_string())
        }
        ProjectKind::Rust => {
            let _ = run_quiet(&dir, "cargo", &["init", "--name", name]);
            None
        }
        ProjectKind::Node => {
            let _ = std::fs::write(
                dir.join("package.json"),
                format!(r#"{{"name":"{}","version":"0.1.0","scripts":{{}}}}"#, name),
            );
            None
        }
        ProjectKind::Docker => {
            let _ = std::fs::write(
                dir.join("docker-compose.yml"),
                "services:\n  app:\n    image: nginx:latest\n",
            );
            None
        }
        ProjectKind::Kubernetes => {
            let _ = std::fs::create_dir_all(dir.join("manifests"));
            let _ = std::fs::write(
                dir.join("manifests").join("deployment.yaml"),
                "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: example\n",
            );
            None
        }
        ProjectKind::Generic => None,
    };

    // README
    let readme = format!(
        "# {}\n\n{}\n\nCreated with TUI-OP-HUB.\n",
        name,
        kind.description()
    );
    let _ = std::fs::write(dir.join("README.md"), readme);

    if git_initialized {
        let _ = run_quiet(&dir, "git", &["add", "."]);
        let _ = run_quiet(&dir, "git", &["commit", "-m", "init"]);
    }

    Ok(CreatedProject {
        path: dir,
        git_initialized,
        env_cmd,
    })
}

/// Run a command quietly (returns true on success).
fn run_quiet(dir: &Path, program: &str, args: &[&str]) -> bool {
    match std::process::Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
    {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_kinds_are_complete() {
        assert_eq!(ProjectKind::all().len(), 6);
        for k in ProjectKind::all() {
            assert!(ProjectKind::from_name(k.name()).is_some());
        }
        assert!(ProjectKind::from_name("nonexistent").is_none());
    }

    #[test]
    fn editors_are_complete() {
        assert_eq!(ProjectEditor::all().len(), 7);
        for e in ProjectEditor::all() {
            assert!(ProjectEditor::from_name(e.name()).is_some());
        }
        assert!(ProjectEditor::from_name("nope").is_none());
    }

    #[test]
    fn editor_open_commands_are_correct() {
        assert_eq!(ProjectEditor::Neovim.open_command("/tmp/x"), "nvim /tmp/x");
        assert!(ProjectEditor::Lazygit
            .open_command("p")
            .contains("lazygit -p p"));
        assert!(ProjectEditor::Lazydocker
            .open_command("p")
            .contains("lazydocker -p p"));
        assert!(ProjectEditor::Yazi.open_command("p").contains("yazi p"));
        assert!(ProjectEditor::VsCode.open_command("p").contains("code p"));
        assert!(ProjectEditor::Helix.open_command("p").contains("hx p"));
    }

    #[test]
    fn create_python_project_with_git() {
        if !crate::keygen::which("git") {
            return;
        }
        let base = std::env::temp_dir().join(format!("tui-proj-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let result = create_project_directory(&base, "myapp", &ProjectKind::Python, false).unwrap();
        assert!(result.path.join(".git").exists(), "git repo created");
        assert!(result.path.join("README.md").exists());
        assert!(result.path.join(".gitignore").exists());
        assert!(result
            .env_cmd
            .as_deref()
            .unwrap()
            .contains("source venv/bin/activate"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn create_docker_project() {
        if !crate::keygen::which("git") {
            return;
        }
        let base = std::env::temp_dir().join(format!("tui-proj-d-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let result = create_project_directory(&base, "dock", &ProjectKind::Docker, false).unwrap();
        assert!(result.path.join("docker-compose.yml").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn create_kubernetes_project() {
        if !crate::keygen::which("git") {
            return;
        }
        let base = std::env::temp_dir().join(format!("tui-proj-k-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let result =
            create_project_directory(&base, "k8s", &ProjectKind::Kubernetes, false).unwrap();
        assert!(result.path.join("manifests/deployment.yaml").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_existing_directory() {
        let base = std::env::temp_dir().join(format!("tui-proj-r-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("exists")).unwrap();
        let r = create_project_directory(&base, "exists", &ProjectKind::Generic, false);
        assert!(
            r.is_err(),
            "existing dir must be rejected without overwrite"
        );
        // With the overwrite option the existing folder is reused (US-PROJ)
        std::fs::write(base.join("exists").join("keep.txt"), "user data").unwrap();
        let ok = create_project_directory(&base, "exists", &ProjectKind::Generic, true);
        assert!(ok.is_ok(), "overwrite=true must reuse the existing dir");
        assert_eq!(
            std::fs::read_to_string(base.join("exists").join("keep.txt")).unwrap(),
            "user data",
            "existing files must survive the merge"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
