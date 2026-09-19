//! Privilege escalation (sudo / doas / su) for running commands as root.
//!
//! Uses the system's own privilege tools — no reimplementation. When the tool
//! requires a password, the TUI shows a secure input popup and the password
//! is piped via stdin (`sudo -S`). The password is never stored or logged.

use crate::error::{AppError, AppResult};

/// The privilege escalation tool in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivTool {
    Sudo,
    Doas,
    Su,
}

impl PrivTool {
    pub fn name(&self) -> &'static str {
        match self {
            PrivTool::Sudo => "sudo",
            PrivTool::Doas => "doas",
            PrivTool::Su => "su",
        }
    }

    /// True when this tool supports piping the password via stdin.
    pub fn supports_stdin_password(&self) -> bool {
        matches!(self, PrivTool::Sudo)
    }
}

/// Detect the first available privilege escalation tool: sudo > doas > su.
pub fn detect_priv_tool() -> Option<PrivTool> {
    if crate::keygen::which("sudo") {
        Some(PrivTool::Sudo)
    } else if crate::keygen::which("doas") {
        Some(PrivTool::Doas)
    } else if crate::keygen::which("su") {
        Some(PrivTool::Su)
    } else {
        None
    }
}

/// Check whether the tool is configured with NOPASSWD (no password prompt
/// needed). Uses `sudo -n true` which fails if a password is required.
pub fn needs_password(tool: &PrivTool) -> bool {
    match tool {
        PrivTool::Sudo => {
            let out = std::process::Command::new("sudo")
                .args(["-n", "true"])
                .output();
            match out {
                Ok(o) => !o.status.success(),
                Err(_) => true,
            }
        }
        _ => true,
    }
}

/// Build the command invocation for running `command` with elevated privileges.
/// Returns (program, args, stdin_password).
pub fn build_invocation(
    tool: &PrivTool,
    command: &str,
    password: Option<&str>,
) -> (String, Vec<String>, Option<String>) {
    match tool {
        PrivTool::Sudo => {
            if password.is_some() {
                (
                    "sudo".to_string(),
                    vec!["-S".to_string(), command.to_string()],
                    password.map(|p| p.to_string()),
                )
            } else {
                ("sudo".to_string(), vec![command.to_string()], None)
            }
        }
        PrivTool::Doas => ("doas".to_string(), vec![command.to_string()], None),
        PrivTool::Su => (
            "sh".to_string(),
            vec![
                "-c".to_string(),
                format!("su -c \"{}\"", command.replace('"', "\\\"")),
            ],
            None,
        ),
    }
}

/// Run a command with elevated privileges. If `password` is provided and the
/// tool supports stdin passwords, it is piped to `sudo -S`.
pub async fn run_privileged(
    tool: &PrivTool,
    command: &str,
    password: Option<&str>,
) -> AppResult<std::process::Output> {
    let (program, args, stdin_pass) = build_invocation(tool, command, password);
    let mut cmd = tokio::process::Command::new(&program);
    for arg in &args {
        cmd.arg(arg);
    }
    if let Some(pass) = &stdin_pass {
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Other(format!("spawn failed: {e}")))?;
        if let Some(stdin) = child.stdin.as_mut() {
            use tokio::io::AsyncWriteExt;
            stdin
                .write_all(format!("{}\n", pass).as_bytes())
                .await
                .map_err(|e| AppError::Other(format!("write password: {e}")))?;
        }
        child
            .wait_with_output()
            .await
            .map_err(|e| AppError::Other(format!("wait failed: {e}")))
    } else {
        cmd.output()
            .await
            .map_err(|e| AppError::Other(format!("spawn failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_a_tool() {
        // Environment-independent (D97): the Nix build sandbox has no sudo/doas
        // on PATH — only assert Some when a tool is actually installed.
        let has_tool = ["sudo", "doas", "su"]
            .iter()
            .any(|t| crate::keygen::which(t));
        if has_tool {
            assert!(detect_priv_tool().is_some());
        } else {
            assert!(detect_priv_tool().is_none());
        }
    }

    #[test]
    fn tool_names_are_correct() {
        assert_eq!(PrivTool::Sudo.name(), "sudo");
        assert_eq!(PrivTool::Doas.name(), "doas");
        assert_eq!(PrivTool::Su.name(), "su");
        assert!(PrivTool::Sudo.supports_stdin_password());
        assert!(!PrivTool::Doas.supports_stdin_password());
    }

    #[test]
    fn build_invocation_sudo_nopasswd() {
        let (program, args, pass) = build_invocation(&PrivTool::Sudo, "apt update", None);
        assert_eq!(program, "sudo");
        assert_eq!(args, vec!["apt update".to_string()]);
        assert!(pass.is_none());
    }

    #[test]
    fn build_invocation_sudo_with_password() {
        let (program, args, pass) = build_invocation(&PrivTool::Sudo, "apt update", Some("secret"));
        assert_eq!(program, "sudo");
        assert_eq!(args, vec!["-S".to_string(), "apt update".to_string()]);
        assert_eq!(pass.as_deref(), Some("secret"));
    }

    #[test]
    fn build_invocation_doas() {
        let (program, args, pass) =
            build_invocation(&PrivTool::Doas, "systemctl restart nginx", None);
        assert_eq!(program, "doas");
        assert_eq!(args, vec!["systemctl restart nginx".to_string()]);
        assert!(pass.is_none());
    }

    #[test]
    fn build_invocation_su() {
        let (program, args, _) = build_invocation(&PrivTool::Su, "pacman -Syu", None);
        assert_eq!(program, "sh");
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("pacman -Syu"));
        assert!(args[1].contains("su -c"));
    }
}
