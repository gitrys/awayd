//! SPDX-License-Identifier: GPL-3.0-or-later
//! Wire format for `Scheduler1.Status()` JSON (daemon serializes, clients parse).

use serde::{Deserialize, Serialize};

pub const STATUS_SCHEMA_VERSION: u32 = 1;

/// `StatusSnapshot::source` when the tick failed: nothing was applied and `work_mode` is
/// unknown (shown as an error in the tray instead of "online").
pub const SOURCE_ERROR: &str = "error";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastApplyRecord {
    pub ok: bool,
    pub work_mode: bool,
    pub source: String,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_error: Option<String>,
}

/// JSON body returned by `Scheduler1.Status()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub ok: bool,
    pub schema: u32,
    pub work_mode: bool,
    pub source: String,
    pub mode_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_wake: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_apply: Option<LastApplyRecord>,
    #[serde(default)]
    pub errors: Vec<String>,
    /// Version of the daemon that produced this status. Empty when sent by an older daemon.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            ok: false,
            schema: STATUS_SCHEMA_VERSION,
            work_mode: false,
            source: "schedule".into(),
            mode_kind: "schedule".into(),
            next_wake: None,
            last_apply: None,
            errors: vec![],
            version: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_without_version_still_parses() {
        // JSON as written by daemons before the `version` field existed.
        let old = r#"{"ok":true,"schema":1,"work_mode":false,"source":"schedule","mode_kind":"schedule","errors":[]}"#;
        let s: StatusSnapshot = serde_json::from_str(old).unwrap();
        assert_eq!(s.version, "");
    }

    #[test]
    fn version_roundtrips_and_is_omitted_when_empty() {
        let s = StatusSnapshot {
            version: "0.1.0".into(),
            ..StatusSnapshot::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""version":"0.1.0""#), "{json}");
        let back: StatusSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, "0.1.0");

        let empty = serde_json::to_string(&StatusSnapshot::default()).unwrap();
        assert!(!empty.contains("version"), "{empty}");
    }
}
