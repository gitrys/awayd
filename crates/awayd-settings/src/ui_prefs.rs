//! SPDX-License-Identifier: GPL-3.0-or-later
//! Persisted window size, position, UI zoom and theme for awayd-settings.

use crate::theme::ThemeChoice;
use awayd_core::config::paths;
use serde::{Deserialize, Serialize};

const DEFAULT_WIDTH: f32 = 960.0;
const DEFAULT_HEIGHT: f32 = 680.0;
const DEFAULT_ZOOM: f32 = 1.0;
/// Smallest window (native points): sidebar plus one card column without squeezed text. The
/// height stays well below laptop screens (768 px); taller pages scroll.
pub const MIN_WIDTH: f32 = 896.0;
const MAX_WIDTH: f32 = 1800.0;
pub const MIN_HEIGHT: f32 = 640.0;
const MAX_HEIGHT: f32 = 1200.0;
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
    /// Missing in files written before the theme setting existed: follow Windows.
    #[serde(default)]
    pub theme: ThemeChoice,
}

impl Default for SettingsUiPrefs {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            zoom: DEFAULT_ZOOM,
            pos_x: None,
            pos_y: None,
            theme: ThemeChoice::System,
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
        awayd_core::atomic_fs::atomic_write(&path, json.as_bytes())
            .map_err(|e| format!("write ui prefs: {e}"))
    }

    fn clamp(&mut self) {
        // The minimum also applies to a saved size: Windows enforces it only while resizing,
        // a window created smaller stays smaller.
        self.width = self.width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.height = self.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
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
        assert_eq!(p.theme, ThemeChoice::System);
    }

    #[test]
    fn saved_size_is_raised_to_the_minimum() {
        let mut p: SettingsUiPrefs =
            serde_json::from_str(r#"{"width":700.0,"height":500.0,"zoom":1.0}"#).unwrap();
        p.clamp();
        assert_eq!((p.width, p.height), (MIN_WIDTH, MIN_HEIGHT));
    }

    #[test]
    fn file_without_theme_follows_windows() {
        let old = r#"{"width":580.0,"height":480.0,"zoom":1.0,"pos_x":10.0,"pos_y":20.0}"#;
        let p: SettingsUiPrefs = serde_json::from_str(old).unwrap();
        assert_eq!(p.theme, ThemeChoice::System);
        assert_eq!(p.pos_x, Some(10.0));
    }

    #[test]
    fn theme_round_trips() {
        let p = SettingsUiPrefs {
            theme: ThemeChoice::Dark,
            ..Default::default()
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""theme":"dark""#), "{json}");
        let back: SettingsUiPrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.theme, ThemeChoice::Dark);
    }
}
