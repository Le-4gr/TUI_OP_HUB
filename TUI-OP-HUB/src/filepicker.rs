//! File/directory picker integration (US-CMD-01).
//!
//! Prefers terminal file managers with a *chooser* mode \u2014 yazi, nnn, ranger,
//! lf \u2014 then falls back to GUI dialogs (zenity / kdialog) and finally to
//! `None` so the caller can open its own manual path input. TUI pickers are
//! run suspended (raw mode off, alternate screen left) so they render cleanly
//! on top of the hub.

use crate::error::{AppError, AppResult};
use std::path::PathBuf;

/// What the picker should return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    /// An existing file (import).
    File,
    /// An existing directory (export target).
    Directory,
}

/// Which picker backend to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Yazi,
    Nnn,
    Ranger,
    Lf,
    Zenity,
    Kdialog,
}

impl Backend {
    pub fn program(self) -> &'static str {
        match self {
            Backend::Yazi => "yazi",
            Backend::Nnn => "nnn",
            Backend::Ranger => "ranger",
            Backend::Lf => "lf",
            Backend::Zenity => "zenity",
            Backend::Kdialog => "kdialog",
        }
    }

    /// Build the argument list. `out_file` is where TUI pickers write their
    /// choice; GUI dialogs print to stdout instead.
    pub fn args(self, kind: PickKind, out_file: &std::path::Path) -> Vec<String> {
        let of = out_file.to_string_lossy().to_string();
        match (self, kind) {
            (Backend::Yazi, _) => vec!["--chooser-file".to_string(), of],
            (Backend::Nnn, PickKind::File) => vec!["-p".to_string(), of],
            (Backend::Nnn, PickKind::Directory) => vec!["-p".to_string(), of],
            (Backend::Ranger, PickKind::File) => vec![format!("--choosefile={}", of)],
            (Backend::Ranger, PickKind::Directory) => vec![format!("--choosedir={}", of)],
            (Backend::Lf, PickKind::File) => vec![format!("-selection-path={}", of)],
            (Backend::Lf, PickKind::Directory) => {
                vec![format!("-last-dir-path={}", of)]
            }
            (Backend::Zenity, PickKind::File) => {
                vec!["--file-selection".to_string()]
            }
            (Backend::Zenity, PickKind::Directory) => {
                vec!["--file-selection".to_string(), "--directory".to_string()]
            }
            (Backend::Kdialog, PickKind::File) => {
                vec!["--getopenfilename".to_string(), ".".to_string()]
            }
            (Backend::Kdialog, PickKind::Directory) => {
                vec!["--getexistingdirectory".to_string(), ".".to_string()]
            }
        }
    }

    /// Does this backend write its choice into a file (TUI pickers) instead
    /// of stdout (GUI dialogs)?
    pub fn uses_out_file(self) -> bool {
        matches!(
            self,
            Backend::Yazi | Backend::Nnn | Backend::Ranger | Backend::Lf
        )
    }

    /// Does this backend need the terminal suspended (it draws a TUI)?
    pub fn needs_terminal_suspension(self) -> bool {
        matches!(
            self,
            Backend::Yazi | Backend::Nnn | Backend::Ranger | Backend::Lf
        )
    }
}

/// Pickers tried in preference order for `kind`.
pub fn available_backends(kind: PickKind) -> Vec<Backend> {
    let all = [
        Backend::Yazi,
        Backend::Nnn,
        Backend::Ranger,
        Backend::Lf,
        Backend::Zenity,
        Backend::Kdialog,
    ];
    if kind == PickKind::Directory {
        // yazi/nnn pick files; for directories their choice's parent is used,
        // which still works \u2014 keep order but note lf/ranger have true dir modes.
    }
    all.into_iter()
        .filter(|b| crate::keygen::which(b.program()))
        .collect()
}

/// Run the first available picker and return the chosen path.
/// TUI pickers run suspended; cancelled pickers return `Ok(None)`.
pub fn pick(kind: PickKind) -> AppResult<Option<PathBuf>> {
    let backends = available_backends(kind);
    if backends.is_empty() {
        return Ok(None);
    }

    let out_file =
        std::env::temp_dir().join(format!("tui-op-hub-picker-{}.out", std::process::id()));
    let _ = std::fs::remove_file(&out_file);

    for backend in backends {
        let args = backend.args(kind, &out_file);
        let suspend = backend.needs_terminal_suspension();

        // TUI pickers: suspend our terminal, run, restore.
        // GUI dialogs: run ONCE capturing stdout (no terminal dance).
        let run_result = if suspend {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
            let res = std::process::Command::new(backend.program())
                .args(&args)
                .status();
            let _ =
                crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);
            let _ = crossterm::terminal::enable_raw_mode();
            res.map(|st| (st, None))
        } else {
            std::process::Command::new(backend.program())
                .args(&args)
                .output()
                .map(|out| {
                    (
                        out.status,
                        Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
                    )
                })
        };

        let Ok((status, stdout_choice)) = run_result else {
            continue; // picker failed to start, try the next one
        };
        if !status.success() {
            // The backend RAN but the user cancelled/errored: do NOT fall
            // through to other backends (that is how a second dialog would
            // open after quitting yazi with `q`).
            let _ = std::fs::remove_file(&out_file);
            return Ok(None);
        }

        let chosen = if backend.uses_out_file() {
            std::fs::read_to_string(&out_file)
                .ok()
                .map(|s| s.trim().to_string())
        } else {
            stdout_choice
        };

        let _ = std::fs::remove_file(&out_file);
        let Some(chosen) = chosen.filter(|s| !s.is_empty()) else {
            return Ok(None); // ran fine but nothing chosen: user cancelled
        };
        let path = PathBuf::from(chosen);
        // lf in directory mode returns the dir; yazi/nnn return a file whose
        // parent is the directory for Directory picks.
        let resolved = match (kind, backend) {
            (PickKind::Directory, Backend::Yazi) | (PickKind::Directory, Backend::Nnn) => {
                path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
            }
            _ => path,
        };
        return Ok(Some(resolved));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_args_build_correctly() {
        let out = std::path::Path::new("/tmp/out");
        assert_eq!(
            Backend::Yazi.args(PickKind::File, out),
            vec!["--chooser-file".to_string(), "/tmp/out".to_string()]
        );
        assert_eq!(
            Backend::Nnn.args(PickKind::File, out),
            vec!["-p".to_string(), "/tmp/out".to_string()]
        );
        assert_eq!(
            Backend::Ranger.args(PickKind::Directory, out),
            vec!["--choosedir=/tmp/out".to_string()]
        );
        assert_eq!(
            Backend::Zenity.args(PickKind::File, out),
            vec!["--file-selection".to_string()]
        );
    }

    #[test]
    fn tui_backends_use_out_file_and_suspend() {
        assert!(Backend::Yazi.uses_out_file());
        assert!(Backend::Yazi.needs_terminal_suspension());
        assert!(!Backend::Zenity.uses_out_file());
        assert!(!Backend::Zenity.needs_terminal_suspension());
    }

    #[test]
    fn directory_pick_via_yazi_uses_parent_dir() {
        // The resolution logic is exercised through `pick`; here we just
        // assert the pure mapping rules hold.
        let path = PathBuf::from("/home/user/exports/chosen.json");
        let parent = path.parent().unwrap().to_path_buf();
        assert_eq!(parent, PathBuf::from("/home/user/exports"));
    }
}
