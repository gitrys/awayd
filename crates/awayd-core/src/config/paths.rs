//! SPDX-License-Identifier: GPL-3.0-or-later
//! Portable and per-user path resolution for awayd on Windows.

use std::path::{Path, PathBuf};

const PORTABLE_MARKER: &str = "portable.marker";

/// True when `AWAYD_PORTABLE=1` or `portable.marker` sits next to the current exe.
pub fn is_portable() -> bool {
    if std::env::var("AWAYD_PORTABLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return true;
    }
    portable_marker_path().is_file()
}

fn exe_parent() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

fn portable_marker_path() -> PathBuf {
    exe_parent()
        .map(|p| p.join(PORTABLE_MARKER))
        .unwrap_or_else(|| PathBuf::from(PORTABLE_MARKER))
}

fn app_data_root() -> PathBuf {
    if is_portable() {
        return exe_parent().unwrap_or_else(|| PathBuf::from("."));
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("awayd"))
        .unwrap_or_else(|| PathBuf::from(".").join("awayd"))
}

/// `{root}/config` — portable: beside exe; else `%APPDATA%/awayd/config`.
pub fn config_dir() -> PathBuf {
    app_data_root().join("config")
}

/// `{root}/state`
pub fn state_dir() -> PathBuf {
    app_data_root().join("state")
}

/// `{root}/state/logs` — rotating NDJSON daemon logs.
pub fn logs_dir() -> PathBuf {
    state_dir().join("logs")
}

pub fn current_log_file() -> PathBuf {
    logs_dir().join("awayd.log")
}

/// `{root}/data`
pub fn data_dir() -> PathBuf {
    app_data_root().join("data")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn state_file() -> PathBuf {
    state_dir().join("state.json")
}

/// Settings GUI window/zoom preferences (`settings_ui.json`).
pub fn settings_ui_prefs_file() -> PathBuf {
    state_dir().join("settings_ui.json")
}

/// `{data}/holidays` (Nager.Date cache).
pub fn holidays_dir() -> PathBuf {
    data_dir().join("holidays")
}

/// Ensure config/state/data directories exist.
pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    std::fs::create_dir_all(state_dir())?;
    std::fs::create_dir_all(holidays_dir())?;
    std::fs::create_dir_all(logs_dir())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_env_flag() {
        std::env::set_var("AWAYD_PORTABLE", "1");
        assert!(is_portable());
        std::env::remove_var("AWAYD_PORTABLE");
    }
}
