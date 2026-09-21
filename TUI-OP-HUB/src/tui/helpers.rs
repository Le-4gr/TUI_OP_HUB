//! Free helper functions for the TUI screens (paths, formatting, terminal
//! window spawning). Extracted from `modern_app.rs` to keep it focused on the
//! app state machine.

/// The pre-filled path for the import popup.
pub(crate) fn default_bundle_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/tui-op-hub-export.json")
}

/// Expand a leading `~` (or `~/`) to `$HOME`.
pub(crate) fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        if !home.is_empty() {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

/// Human-readable byte rate: `1.2K`, `3.4M`, ... (per refresh interval).
pub(crate) fn humans(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1}M", b / 1024.0 / 1024.0)
    } else if b >= 1024.0 {
        format!("{:.1}K", b / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

/// `Some(s)` when `s` is non-empty (after trim), else `None`.
pub(crate) fn some_if_not_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// (program, args-before-shell) for known terminal emulators, in preference order.
pub(crate) const TERMINAL_EMULATORS: &[(&str, &[&str])] = &[
    ("alacritty", &["-e"]),
    ("kitty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["-e"]),
    ("tilix", &["-e"]),
    ("foot", &[]),
    ("xterm", &["-e"]),
    ("st", &["-e"]),
    ("uxterm", &["-e"]),
];

/// Find a terminal emulator command to open a NEW terminal window running
/// the user's shell. Probes `$TUI_OP_HUB_TERMINAL` / `$TERMINAL` first, then
/// falls back through common Linux terminal emulators. Returns
/// `(program, args)` or `None` if no emulator is found.
///
/// Detached spawn: the TUI keeps running while the new window is open.
pub(crate) fn terminal_window_command() -> Option<(String, Vec<String>)> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // User override (full command with args allowed via shell-style split)
    for var in ["TUI_OP_HUB_TERMINAL", "TERMINAL"] {
        if let Ok(t) = std::env::var(var) {
            let t = t.trim().to_string();
            if !t.is_empty() && which_program(&t) {
                // Try `-e <shell>` (most emulators), then plain `<shell>`
                return Some((t, vec!["-e".to_string(), shell]));
            }
        }
    }

    const EMULATORS: &[(&str, &[&str])] = TERMINAL_EMULATORS;

    for (prog, prefix) in EMULATORS {
        if which_program(prog) {
            let mut args: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            args.push(shell);
            return Some((prog.to_string(), args));
        }
    }
    None
}

/// Like [`terminal_window_command`], but runs `command` (optionally after
/// `cd cwd`) instead of the user's shell — used by the quick-launch row.
pub(crate) fn terminal_window_command_for(
    command: &str,
    cwd: Option<&str>,
) -> Option<(String, Vec<String>)> {
    let full = match cwd {
        Some(dir) => format!("cd '{}' && {}", dir, command),
        None => command.to_string(),
    };

    // User override first
    for var in ["TUI_OP_HUB_TERMINAL", "TERMINAL"] {
        if let Ok(t) = std::env::var(var) {
            let t = t.trim().to_string();
            if !t.is_empty() && which_program(&t) {
                return Some((
                    t,
                    vec!["-e".to_string(), "sh".to_string(), "-c".to_string(), full],
                ));
            }
        }
    }

    for (prog, prefix) in TERMINAL_EMULATORS {
        if which_program(prog) {
            let mut args: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            args.push("sh".to_string());
            args.push("-c".to_string());
            args.push(full.clone());
            return Some((prog.to_string(), args));
        }
    }
    None
}

/// Poor-man's `which`: check whether `prog` resolves to an executable file
/// on `$PATH` (or is an absolute path that exists).
pub(crate) fn which_program(prog: &str) -> bool {
    if prog.contains('/') {
        return std::path::Path::new(prog).is_file();
    }
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|p| !p.is_empty())
        .map(std::path::PathBuf::from)
        .map(|dir| dir.join(prog))
        .any(|candidate| candidate.is_file())
}

/// Default directory for new project workspaces: `$HOME/projects`.
pub(crate) fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("projects"))
        .unwrap_or_else(|_| std::path::PathBuf::from("projects"))
}

/// D118: does this private key file have a same-stem `.pub` next to it?
pub(crate) fn pub_path_if(priv_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = priv_path.to_string_lossy().trim_end_matches(".priv").to_string();
    let cand = std::path::PathBuf::from(format!("{}.pub", priv_path.display()));
    if cand.is_file() {
        Some(cand)
    } else {
        let _ = stem;
        None
    }
}

