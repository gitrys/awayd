//! SPDX-License-Identifier: GPL-3.0-or-later
//! Resolve and spawn awayd component binaries (sibling-of-exe, PATH, env override).

use std::path::{Path, PathBuf};
use std::process::Child;

const SETTINGS_BIN: &str = "awayd-settings";
const SETTINGS_ENV: &str = "AWAYD_SETTINGS";

fn exe_name(component: &str) -> String {
    if cfg!(windows) && !component.ends_with(".exe") {
        format!("{component}.exe")
    } else {
        component.to_string()
    }
}

/// Resolve path to an awayd component executable.
///
/// Order: `AWAYD_SETTINGS` (settings only via [`spawn_settings`]), sibling of
/// [`std::env::current_exe`], then `PATH`.
pub fn resolve_awayd_binary(component: &str) -> PathBuf {
    if component == SETTINGS_BIN {
        if let Ok(p) = std::env::var(SETTINGS_ENV) {
            let path = PathBuf::from(p);
            if is_executable(&path) {
                return path;
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sibling = parent.join(exe_name(component));
            if is_executable(&sibling) {
                return sibling;
            }
        }
    }

    if let Some(p) = find_in_path(component) {
        return p;
    }

    PathBuf::from(component)
}

/// Like [`resolve_awayd_binary`] but only searches under `dir` (for tests).
pub fn resolve_in_dir(dir: &Path, component: &str) -> PathBuf {
    let candidate = dir.join(component);
    if is_executable(&candidate) {
        candidate
    } else {
        PathBuf::from(component)
    }
}

/// Spawn the settings GUI binary.
pub fn spawn_settings() -> std::io::Result<Child> {
    let path = resolve_awayd_binary(SETTINGS_BIN);
    std::process::Command::new(&path).spawn()
}

/// Start the settings on page `page` (`awayd-settings --page <page>`, e.g. "services").
pub fn spawn_settings_page(page: &str) -> std::io::Result<Child> {
    let path = resolve_awayd_binary(SETTINGS_BIN);
    std::process::Command::new(&path)
        .args(["--page", page])
        .spawn()
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .ok()
            .map(|m| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    m.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    let _ = m;
                    true
                }
            })
            .unwrap_or(false)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(exe_name(name));
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolve_in_dir_finds_sibling() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin = dir.path().join("awayd-settings");
        fs::write(&bin, b"").expect("write");
        let mut perms = fs::metadata(&bin).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin, perms).expect("chmod");
        let got = resolve_in_dir(dir.path(), "awayd-settings");
        assert_eq!(got, bin);
    }
}
