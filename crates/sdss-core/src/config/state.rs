//! SPDX-License-Identifier: GPL-3.0-or-later
//! Runtime `state.json`: override + last apply snapshot.

use serde::{Deserialize, Serialize};

pub use crate::status::LastApplyRecord;

pub const STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub schema_version: u32,
    #[serde(default)]
    pub r#override: Option<OverrideRecord>,
    #[serde(default)]
    pub last_apply: Option<LastApplyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideRecord {
    pub version: u32,
    /// When true, user wants "invisible" work mode.
    pub work_mode: bool,
    pub expires_at: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            r#override: None,
            last_apply: None,
        }
    }
}
