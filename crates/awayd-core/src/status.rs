//! SPDX-License-Identifier: GPL-3.0-or-later
//! Wire format for `Scheduler1.Status()` JSON (daemon serializes, clients parse).

use serde::{Deserialize, Serialize};

pub const STATUS_SCHEMA_VERSION: u32 = 1;

/// `StatusSnapshot::source` when the tick failed: nothing was applied and `work_mode` is
/// unknown (shown as an error in the tray instead of "online").
pub const SOURCE_ERROR: &str = "error";

/// What happened to one backend in an apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendApplyStatus {
    Sent,
    /// Deliberately not applied (no credential, program not running).
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendApplyRecord {
    /// Backend id (`awayd_core::backends`).
    pub id: String,
    pub status: BackendApplyStatus,
    /// Error or skip reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "LastApplyRecordWire")]
pub struct LastApplyRecord {
    pub ok: bool,
    pub work_mode: bool,
    pub source: String,
    pub at: String,
    /// Backends applied in this run (backends not attempted are not listed).
    #[serde(default)]
    pub backends: Vec<BackendApplyRecord>,
}

impl LastApplyRecord {
    /// First backend that failed, for one-line displays.
    pub fn first_failure(&self) -> Option<&BackendApplyRecord> {
        self.backends
            .iter()
            .find(|b| b.status == BackendApplyStatus::Failed)
    }
}

/// Read format: also accepts `discord_error` / `steam_error` written by awayd 0.1.0
/// (`state.json`, status JSON) and turns them into failed backend entries.
#[derive(Deserialize)]
struct LastApplyRecordWire {
    ok: bool,
    work_mode: bool,
    source: String,
    at: String,
    #[serde(default)]
    backends: Vec<BackendApplyRecord>,
    #[serde(default)]
    discord_error: Option<String>,
    #[serde(default)]
    steam_error: Option<String>,
}

impl From<LastApplyRecordWire> for LastApplyRecord {
    fn from(w: LastApplyRecordWire) -> Self {
        let mut backends = w.backends;
        if backends.is_empty() {
            for (id, error) in [("discord", w.discord_error), ("steam", w.steam_error)] {
                if let Some(message) = error {
                    backends.push(BackendApplyRecord {
                        id: id.into(),
                        status: BackendApplyStatus::Failed,
                        message: Some(message),
                    });
                }
            }
        }
        Self {
            ok: w.ok,
            work_mode: w.work_mode,
            source: w.source,
            at: w.at,
            backends,
        }
    }
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
    fn last_apply_from_0_1_0_is_converted() {
        let old = r#"{"ok":false,"work_mode":true,"source":"schedule","at":"2026-11-22T08:00:00","discord_error":"401 unauthorized"}"#;
        let r: LastApplyRecord = serde_json::from_str(old).unwrap();
        assert_eq!(
            r.backends,
            vec![BackendApplyRecord {
                id: "discord".into(),
                status: BackendApplyStatus::Failed,
                message: Some("401 unauthorized".into()),
            }]
        );
        assert_eq!(r.first_failure().unwrap().id, "discord");

        let ok_old = r#"{"ok":true,"work_mode":false,"source":"schedule","at":"x"}"#;
        let r: LastApplyRecord = serde_json::from_str(ok_old).unwrap();
        assert!(r.backends.is_empty());
        assert!(r.first_failure().is_none());
    }

    #[test]
    fn last_apply_backends_roundtrip_without_legacy_fields() {
        let r = LastApplyRecord {
            ok: true,
            work_mode: false,
            source: "schedule".into(),
            at: "x".into(),
            backends: vec![
                BackendApplyRecord {
                    id: "steam".into(),
                    status: BackendApplyStatus::Skipped,
                    message: Some("steam is not running".into()),
                },
                BackendApplyRecord {
                    id: "discord".into(),
                    status: BackendApplyStatus::Sent,
                    message: None,
                },
            ],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains(r#""status":"skipped""#), "{json}");
        assert!(!json.contains("discord_error"), "{json}");
        let back: LastApplyRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.backends, r.backends);
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
