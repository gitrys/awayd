//! SPDX-License-Identifier: GPL-3.0-or-later
//! Wire format for `Scheduler1.Status()` JSON (daemon serializes, clients parse).

use serde::{Deserialize, Serialize};

pub const STATUS_SCHEMA_VERSION: u32 = 1;

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
        }
    }
}
