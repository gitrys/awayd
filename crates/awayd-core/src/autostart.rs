//! SPDX-License-Identifier: GPL-3.0-or-later
//! Start awayd at login: value `awayd` under `HKCU\...\CurrentVersion\Run`.
//!
//! Shared by `awayctl install-autostart` and the settings window.

use std::io;
use std::path::Path;

use anyhow::{bail, Context, Result};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "awayd";

/// Current login autostart entry, compared with the awayd.exe it should start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartState {
    /// No entry.
    Off,
    /// Entry starts this executable. `quoted == false`: written by an older version without
    /// quotes, which breaks for paths containing spaces.
    On { quoted: bool },
    /// Entry starts another executable (e.g. an old portable folder).
    Elsewhere(String),
}

/// Run-key command line for `exe`: always quoted, so paths with spaces work.
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Executable part of a Run-key command line and whether it was quoted.
fn exe_of(command: &str) -> (String, bool) {
    let t = command.trim();
    match t.strip_prefix('"') {
        Some(rest) => {
            let end = rest.find('"').unwrap_or(rest.len());
            (rest[..end].to_string(), true)
        }
        None => (t.to_string(), false),
    }
}

/// Classify a Run-key value (`None` = no value) for `exe`. Windows paths compare
/// case-insensitively.
pub fn classify(value: Option<&str>, exe: &Path) -> AutostartState {
    let Some(value) = value.filter(|v| !v.trim().is_empty()) else {
        return AutostartState::Off;
    };
    let (path, quoted) = exe_of(value);
    if path.eq_ignore_ascii_case(&exe.to_string_lossy()) {
        AutostartState::On { quoted }
    } else {
        AutostartState::Elsewhere(path)
    }
}

pub fn state(exe: &Path) -> Result<AutostartState> {
    state_in(RUN_KEY, exe)
}

/// Register `exe` (must exist) to start at login.
pub fn install(exe: &Path) -> Result<()> {
    if !exe.is_file() {
        bail!("{} not found", exe.display());
    }
    install_in(RUN_KEY, exe)
}

/// Remove the entry. `Ok(false)` when there was none; any other failure is an error.
pub fn uninstall() -> Result<bool> {
    uninstall_in(RUN_KEY)
}

fn state_in(key: &str, exe: &Path) -> Result<AutostartState> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = match hkcu.open_subkey_with_flags(key, KEY_READ) {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(AutostartState::Off),
        Err(e) => return Err(e).with_context(|| format!("open HKCU\\{key}")),
    };
    match run.get_value::<String, _>(VALUE_NAME) {
        Ok(v) => Ok(classify(Some(&v), exe)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(AutostartState::Off),
        Err(e) => Err(e).with_context(|| format!("read value {VALUE_NAME}")),
    }
}

fn install_in(key: &str, exe: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu
        .create_subkey(key)
        .with_context(|| format!("open HKCU\\{key}"))?;
    run.set_value(VALUE_NAME, &command_for(exe))
        .with_context(|| format!("write value {VALUE_NAME}"))
}

fn uninstall_in(key: &str) -> Result<bool> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = match hkcu.open_subkey_with_flags(key, KEY_WRITE) {
        Ok(k) => k,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("open HKCU\\{key}")),
    };
    match run.delete_value(VALUE_NAME) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).with_context(|| format!("delete value {VALUE_NAME}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Program Files\awayd portable\awayd.exe")
    }

    #[test]
    fn command_is_quoted() {
        assert_eq!(
            command_for(&exe()),
            r#""C:\Program Files\awayd portable\awayd.exe""#
        );
    }

    #[test]
    fn classify_entries() {
        let e = exe();
        assert_eq!(classify(None, &e), AutostartState::Off);
        assert_eq!(classify(Some("  "), &e), AutostartState::Off);
        assert_eq!(
            classify(Some(&command_for(&e)), &e),
            AutostartState::On { quoted: true }
        );
        assert_eq!(
            classify(Some(r"C:\Program Files\awayd portable\awayd.exe"), &e),
            AutostartState::On { quoted: false },
            "old entry without quotes"
        );
        assert_eq!(
            classify(Some(r#""c:\program files\AWAYD PORTABLE\awayd.exe""#), &e),
            AutostartState::On { quoted: true },
            "case-insensitive"
        );
        assert_eq!(
            classify(Some(r#""D:\old\awayd-portable\awayd.exe""#), &e),
            AutostartState::Elsewhere(r"D:\old\awayd-portable\awayd.exe".into())
        );
    }

    /// Temporary key under HKCU\Software, removed on drop; the real Run key is never touched.
    struct TestKey(String);

    impl TestKey {
        fn new() -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            Self(format!(
                r"Software\awayd-test-{}-{nanos}",
                std::process::id()
            ))
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
        }
    }

    #[test]
    fn registry_roundtrip_on_test_key() {
        let key = TestKey::new();
        let e = exe();

        assert_eq!(state_in(&key.0, &e).unwrap(), AutostartState::Off);
        assert!(!uninstall_in(&key.0).unwrap(), "nothing to remove yet");

        install_in(&key.0, &e).unwrap();
        assert_eq!(
            state_in(&key.0, &e).unwrap(),
            AutostartState::On { quoted: true }
        );
        let other = PathBuf::from(r"D:\elsewhere\awayd.exe");
        assert_eq!(
            state_in(&key.0, &other).unwrap(),
            AutostartState::Elsewhere(e.display().to_string())
        );

        assert!(uninstall_in(&key.0).unwrap(), "removed");
        assert_eq!(state_in(&key.0, &e).unwrap(), AutostartState::Off);
        assert!(!uninstall_in(&key.0).unwrap(), "already gone");
    }

    #[test]
    fn install_requires_existing_exe() {
        let err = install(Path::new(r"C:\does-not-exist\awayd.exe")).unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }
}
