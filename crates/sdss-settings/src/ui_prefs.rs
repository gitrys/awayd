//! SPDX-License-Identifier: GPL-3.0-or-later
//! Persisted window size, position, and UI zoom for sdss-settings.

use sdss_core::config::paths;
use serde::{Deserialize, Serialize};

const DEFAULT_WIDTH: f32 = 580.0;
const DEFAULT_HEIGHT: f32 = 480.0;
const DEFAULT_ZOOM: f32 = 1.0;
const MIN_WIDTH: f32 = 400.0;
const MAX_WIDTH: f32 = 1400.0;
const MIN_HEIGHT: f32 = 360.0;
const MAX_HEIGHT: f32 = 1000.0;
const MIN_ZOOM: f32 = 0.75;
const MAX_ZOOM: f32 = 1.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsUiPrefs {
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
    /// Outer window top-left (screen points). None until the user moves the window once.
    #[serde(default)]
    pub pos_x: Option<f32>,
    #[serde(default)]
    pub pos_y: Option<f32>,
}

impl Default for SettingsUiPrefs {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            zoom: DEFAULT_ZOOM,
            pos_x: None,
            pos_y: None,
        }
    }
}

impl SettingsUiPrefs {
    pub fn load() -> Self {
        let path = paths::settings_ui_prefs_file();
        let mut prefs = if path.is_file() {
            match std::fs::read_to_string(&path) {
                Ok(raw) => match serde_json::from_str(&raw) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            "settings_ui.json parse failed, using defaults: {e}"
                        );
                        Self::default()
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        "settings_ui.json read failed, using defaults: {e}"
                    );
                    Self::default()
                }
            }
        } else {
            Self::default()
        };
        prefs.clamp();
        prefs
    }

    pub fn save(&self) -> Result<(), String> {
        let _ = paths::ensure_dirs();
        let path = paths::settings_ui_prefs_file();
        let mut to_save = self.clone();
        to_save.clamp_for_save();
        let json = serde_json::to_string_pretty(&to_save)
            .map_err(|e| format!("serialize ui prefs: {e}"))?;
        sdss_core::atomic_fs::atomic_write(&path, json.as_bytes())
            .map_err(|e| format!("write ui prefs: {e}"))
    }

    fn clamp(&mut self) {
        // Do not enforce minimum width/height on load — that breaks restore of narrow windows.
        self.width = self.width.clamp(1.0, MAX_WIDTH);
        self.height = self.height.clamp(1.0, MAX_HEIGHT);
        self.zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn clamp_for_save(&mut self) {
        self.width = self.width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.height = self.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        self.zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let p = SettingsUiPrefs::default();
        assert_eq!(p.width, DEFAULT_WIDTH);
        assert_eq!(p.zoom, DEFAULT_ZOOM);
        assert!(p.pos_x.is_none());
        assert!(p.pos_y.is_none());
    }
}
