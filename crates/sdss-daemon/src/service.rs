//! SPDX-License-Identifier: GPL-3.0-or-later
//! Scheduler API — shared by named-pipe IPC and in-process tray actions.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Datelike;
use sdss_core::config::paths;
use sdss_core::config::schema::AppConfig;
use sdss_ipc::SchedulerApi;
use tokio::sync::RwLock;

use crate::holidays::{self, load_holiday_nearby_years};
use crate::override_state;
use crate::runtime_config::build_config;
use crate::secret;
use crate::watchers::PokeGate;

/// In-memory cache for `status()`.
#[derive(Clone, Default)]
pub struct StatusCache {
    pub json: String,
    pub work_mode: bool,
    pub source: String,
    pub next_wake: String,
}

pub struct SchedulerService {
    pub status: Arc<RwLock<StatusCache>>,
    pub config_toml: Arc<RwLock<String>>,
    pub poke: Arc<PokeGate>,
    pub shutdown: Arc<tokio::sync::Notify>,
}

impl SchedulerService {
    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

#[async_trait]
impl SchedulerApi for SchedulerService {
    async fn status(&self) -> String {
        self.status.read().await.json.clone()
    }

    async fn get_config_toml(&self) -> String {
        self.config_toml.read().await.clone()
    }

    async fn has_token(&self) -> bool {
        match secret::get_token().await {
            Ok(Some(t)) => !t.trim().is_empty(),
            _ => false,
        }
    }

    async fn list_subdivisions(&self, country: &str) -> Vec<String> {
        holidays::catalogue::subdivisions_for_country(country)
    }

    fn list_available_countries(&self) -> Vec<(String, String)> {
        holidays::sync::list_available_countries_cached_sync().unwrap_or_default()
    }

    async fn holiday_data_status(&self, country: &str) -> (bool, i32, i32, String, String) {
        let s = holidays::sync::holiday_data_status(country);
        (
            s.ready,
            s.year_min,
            s.year_max,
            s.fetched_at,
            s.message,
        )
    }

    fn refresh_holiday_data(&self, country: &str) -> String {
        let cc = country.trim().to_uppercase();
        match holidays::sync::refresh_country_sync(&cc) {
            Ok(()) => format!("ok: refreshed holiday data for {cc}"),
            Err(e) => format!("error: {e}"),
        }
    }

    async fn list_holidays(&self, country: &str, subdiv: &str, year: i32) -> serde_json::Value {
        let (year_min, year_max) = holidays::year_range_in_file(country).unwrap_or((year, year));
        match holidays::list_holidays_for_subdiv(country, subdiv, year) {
            Ok(holidays) => serde_json::json!({
                "year_min": year_min,
                "year_max": year_max,
                "holidays": holidays,
                "error": null,
            }),
            Err(e) => serde_json::json!({
                "year_min": year_min,
                "year_max": year_max,
                "holidays": [],
                "error": e,
            }),
        }
    }

    async fn set_config_toml(&self, toml: &str) -> bool {
        let path = paths::config_file();
        let prev_country = sdss_core::config::load_config(&path)
            .map(|c| c.schedule.country.to_uppercase())
            .unwrap_or_default();
        let app: AppConfig = match toml::from_str(toml) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetConfigToml parse: {e}");
                return false;
            }
        };
        if let Err(e) = crate::runtime_config::validate_app_config(&app) {
            tracing::warn!(target: "sdssd::api", "SetConfigToml validate: {e}");
            return false;
        }
        if let Err(e) = sdss_core::atomic_fs::atomic_write(&path, toml.as_bytes()) {
            tracing::warn!(target: "sdssd::api", "SetConfigToml write: {e}");
            return false;
        }
        *self.config_toml.write().await = toml.to_string();
        sdss_core::logging::apply_capture_level(&app.advanced);
        let new_country = app.schedule.country.to_uppercase();
        if new_country != prev_country {
            std::thread::spawn(move || {
                if let Err(e) = holidays::sync::ensure_country_sync(&new_country) {
                    tracing::warn!(
                        target: "sdss::holidays",
                        country = %new_country,
                        error = %e,
                        "holiday sync after country change failed"
                    );
                }
            });
        }
        self.poke.poke_immediate();
        true
    }

    async fn reload_config(&self) -> bool {
        let path = paths::config_file();
        if let Err(e) = sdss_core::config::load_config(&path) {
            tracing::warn!(target: "sdssd::api", "ReloadConfig parse: {e}");
            return false;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "ReloadConfig read: {e}");
                return false;
            }
        };
        *self.config_toml.write().await = raw;
        self.poke.poke_immediate();
        true
    }

    async fn set_token(&self, token: &str) -> bool {
        if token.trim().is_empty() {
            return false;
        }
        let norm = crate::apply::discord::normalize_token(token);
        match secret::set_token(&norm).await {
            Ok(()) => {
                self.poke.poke_immediate();
                true
            }
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetToken: {e}");
                false
            }
        }
    }

    async fn clear_token(&self) -> bool {
        match secret::clear_token().await {
            Ok(()) => {
                self.poke.poke_immediate();
                true
            }
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "ClearToken: {e}");
                false
            }
        }
    }

    async fn apply_now(&self) -> bool {
        tracing::info!(target: "sdssd::api", "ApplyNow requested");
        self.poke.poke_immediate();
        true
    }

    async fn set_override(&self, mode: &str, ttl_seconds: u32) -> bool {
        let work_mode = match mode.to_ascii_lowercase().as_str() {
            "invisible" => true,
            "online" => false,
            _ => {
                tracing::warn!(target: "sdssd::api", "SetOverride bad mode {mode:?}");
                return false;
            }
        };
        let cfg_path = paths::config_file();
        let app = match sdss_core::config::load_config(&cfg_path) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetOverride load config: {e}");
                return false;
            }
        };
        let work = match build_config(&app, "") {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetOverride build config: {e}");
                return false;
            }
        };
        let now = chrono::Local::now();
        let hol = match load_holiday_nearby_years(
            &app.schedule.country,
            &app.schedule.subdiv,
            now.year(),
        ) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetOverride holidays: {e}");
                return false;
            }
        };
        let exp = override_state::override_expires_at(now, ttl_seconds, &work, &hol);
        let exp_s = exp.format("%Y-%m-%dT%H:%M:%S").to_string();
        let path = paths::state_file();
        let mut st = match sdss_core::config::load_state(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetOverride load state: {e}");
                return false;
            }
        };
        st.r#override = Some(sdss_core::config::state::OverrideRecord {
            version: override_state::OVERRIDE_JSON_VERSION,
            work_mode,
            expires_at: exp_s,
        });
        if let Err(e) = sdss_core::config::save_state(&path, &st) {
            tracing::warn!(target: "sdssd::api", "SetOverride save: {e}");
            return false;
        }
        self.poke.poke_immediate();
        true
    }

    async fn clear_override(&self) -> bool {
        let path = paths::state_file();
        let mut st = match sdss_core::config::load_state(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "sdssd::api", "SetOverride load state: {e}");
                return false;
            }
        };
        st.r#override = None;
        if let Err(e) = sdss_core::config::save_state(&path, &st) {
            tracing::warn!(target: "sdssd::api", "ClearOverride save: {e}");
            return false;
        }
        self.poke.poke_immediate();
        true
    }

    fn quit(&self) {
        tracing::info!(target: "sdssd::api", "Quit requested");
        self.shutdown.notify_waiters();
    }
}
