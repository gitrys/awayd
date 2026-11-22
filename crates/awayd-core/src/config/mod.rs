//! SPDX-License-Identifier: GPL-3.0-or-later
//! Load / save `config.toml` and `state.json`.

pub mod paths;
pub mod schema;
pub mod state;

use crate::atomic_fs;
use crate::schedule;
use anyhow::{Context, Result};
use schema::{AppConfig, CONFIG_SCHEMA_VERSION};
use state::AppState;
use std::path::Path;

pub fn load_config(path: &Path) -> Result<AppConfig> {
    if !path.is_file() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    parse_config(&raw)
}

/// Parse and validate `config.toml` text (same rules as [`load_config`]).
pub fn parse_config(raw: &str) -> Result<AppConfig> {
    let mut c: AppConfig = toml::from_str(raw).with_context(|| "parse config.toml")?;
    schedule::normalize_schedule(&mut c.schedule);
    if c.schema_version != CONFIG_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported config schema_version {} (expected {})",
            c.schema_version,
            CONFIG_SCHEMA_VERSION
        );
    }
    Ok(c)
}

pub fn save_config(path: &Path, cfg: &AppConfig) -> Result<()> {
    let raw = toml::to_string_pretty(cfg).context("serialize config")?;
    atomic_fs::atomic_write(path, raw.as_bytes())?;
    Ok(())
}

pub fn load_state(path: &Path) -> Result<AppState> {
    if !path.is_file() {
        return Ok(AppState::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let st: AppState = serde_json::from_str(&raw).with_context(|| "parse state.json")?;
    if st.schema_version != state::STATE_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported state schema_version {} (expected {}); delete or migrate {:?}",
            st.schema_version,
            state::STATE_SCHEMA_VERSION,
            path
        );
    }
    Ok(st)
}

pub fn save_state(path: &Path, st: &AppState) -> Result<()> {
    let raw = serde_json::to_string_pretty(st)?;
    atomic_fs::atomic_write(path, raw.as_bytes())?;
    Ok(())
}
