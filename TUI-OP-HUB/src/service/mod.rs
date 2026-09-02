//! Service & init-system integration (US-DEP-04).
//!
//! Generates and installs a **systemd user service** so the hub (and its
//! workflow scheduler + API) can run headless in the background, plus a
//! cron line for cron-only systems. No external dependencies: everything
//! is plain string generation, so it works for any init that can consume
//! a unit file or a crontab line.
//!
//! Usage:
//! ```text
//! tui-op-hub --print-unit         # show the unit file on stdout
//! tui-op-hub --install-service    # write + daemon-reload + enable (best effort)
//! tui-op-hub --uninstall-service  # disable + remove (best effort)
//! tui-op-hub --headless           # run without the TUI (daemon mode)
//! ```

use std::path::PathBuf;

/// The systemd unit name.
pub const SERVICE_NAME: &str = "tui-op-hub";

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

/// Where the user unit file lives.
pub fn unit_path() -> PathBuf {
    dirs_home()
        .join(".config/systemd/user")
        .join(format!("{SERVICE_NAME}.service"))
}

/// Resolve the current executable path for `ExecStart`.
fn current_exe() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| "/usr/local/bin/tui-op-hub".to_string())
}

/// Generate the systemd **user** unit file content.
///
/// The service runs the hub headless (no TUI) so the workflow scheduler and
/// REST API stay available in the background. `TUI_OP_HUB_SECRETS_KEY` is
/// intentionally NOT written into the unit; provide it with
/// `systemctl --user set-environment` or an `EnvironmentFile=` instead.
pub fn systemd_user_unit() -> String {
    let exe = current_exe();
    format!(
        r#"[Unit]
Description=TUI-OP-HUB operations hub (headless)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={exe} --headless
Restart=on-failure
RestartSec=3
# Provide the master key without persisting it in the unit file:
#   systemctl --user set-environment TUI_OP_HUB_SECRETS_KEY=<base64-32-bytes>
#Environment=TUI_OP_HUB_SECRETS_KEY=
#EnvironmentFile=%h/.config/tui-op-hub/env

[Install]
WantedBy=default.target
"#
    )
}

/// Generate a cron line that keeps the hub running headless (restarts it if
/// it died). For cron-only systems without systemd.
pub fn cron_line() -> String {
    let exe = current_exe();
    format!("* * * * * pgrep -f '{exe} --headless' >/dev/null || {exe} --headless # tui-op-hub")
}

/// Write the unit file and try to enable the service (best effort: a missing
/// `systemctl` is not an error \u{2014} the file is still written for other inits).
pub fn install_service() -> std::io::Result<PathBuf> {
    let path = unit_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, systemd_user_unit())?;
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", SERVICE_NAME])
        .status();
    Ok(path)
}

/// Disable and remove the unit file (best effort).
pub fn uninstall_service() -> std::io::Result<()> {
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "disable", "--now", SERVICE_NAME])
        .status();
    let path = unit_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_file_contains_required_sections() {
        let unit = systemd_user_unit();
        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("ExecStart="));
        assert!(unit.contains("--headless"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("WantedBy=default.target"));
    }

    #[test]
    fn unit_file_does_not_embed_secrets_key() {
        // The real key must never be persisted into an active directive.
        // (Mentioning it in a comment is fine.)
        let unit = systemd_user_unit();
        let active = unit
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!active.contains("TUI_OP_HUB_SECRETS_KEY="));
    }

    #[test]
    fn cron_line_references_headless_binary() {
        let line = cron_line();
        assert!(line.contains("--headless"));
        assert!(line.starts_with("* * * * *"));
        assert!(line.contains("tui-op-hub"));
    }

    #[test]
    fn unit_path_is_under_systemd_user_dir() {
        let p = unit_path();
        assert!(p.to_string_lossy().contains(".config/systemd/user/"));
        assert!(p.to_string_lossy().ends_with("tui-op-hub.service"));
    }
}
