//! SPDX-License-Identifier: GPL-3.0-or-later
//! Status services ("backends") awayd can manage.
//!
//! The daemon implements each entry as an `apply::Backend`; the settings window, the tray and
//! `awayctl` use this table for names, credential fields and the process watcher. Adding a
//! service means one entry here plus its implementation in the daemon.

/// Static description of one backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendInfo {
    /// Stable id: config key, credential name, log field `backend`, status entries.
    pub id: &'static str,
    pub display_name: &'static str,
    /// Executables whose start triggers a re-apply (process name as in Task Manager).
    pub process_names: &'static [&'static str],
    /// Label of the credential this backend needs (e.g. "Discord token"); `None` = none.
    pub credential_label: Option<&'static str>,
    /// Switched on in a new configuration. Configurations without an entry for this backend
    /// (e.g. from 0.1.0) always treat it as switched on.
    pub enabled_by_default: bool,
    /// Shown next to the switch in the settings window.
    pub risk_note: Option<&'static str>,
}

pub const DISCORD: &str = "discord";
pub const STEAM: &str = "steam";

pub const ALL: &[BackendInfo] = &[
    BackendInfo {
        id: DISCORD,
        display_name: "Discord",
        process_names: &["Discord.exe"],
        credential_label: Some("Discord token"),
        // Off for new installations: automating a user account is very likely against
        // Discord's Terms of Service (README warning).
        enabled_by_default: false,
        risk_note: Some(
            "Uses your Discord user token. This is very likely against Discord's Terms of \
             Service and may get your account restricted. Use at your own risk.",
        ),
    },
    BackendInfo {
        id: STEAM,
        display_name: "Steam",
        process_names: &["steam.exe"],
        credential_label: None,
        enabled_by_default: true,
        risk_note: None,
    },
];

pub fn find(id: &str) -> Option<&'static BackendInfo> {
    ALL.iter().find(|b| b.id == id)
}

/// Backend `id` exists and stores a credential.
pub fn needs_credential(id: &str) -> bool {
    find(id).is_some_and(|b| b.credential_label.is_some())
}

/// Process names of all backends, in table order (watched by the process watcher).
pub fn all_process_names() -> Vec<&'static str> {
    ALL.iter()
        .flat_map(|b| b.process_names.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_safe_as_file_names() {
        for (i, b) in ALL.iter().enumerate() {
            assert!(
                b.id.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                b.id
            );
            assert!(ALL[i + 1..].iter().all(|o| o.id != b.id), "{}", b.id);
        }
    }

    #[test]
    fn lookup_and_credentials() {
        assert_eq!(find(DISCORD).unwrap().display_name, "Discord");
        assert!(find("slack").is_none());
        assert!(needs_credential(DISCORD));
        assert!(!needs_credential(STEAM));
        assert!(!needs_credential("slack"));
        assert_eq!(all_process_names(), vec!["Discord.exe", "steam.exe"]);
    }
}
