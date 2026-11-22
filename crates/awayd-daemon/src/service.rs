//! SPDX-License-Identifier: GPL-3.0-or-later
//! Scheduler API — shared by named-pipe IPC and in-process tray actions.

use std::sync::Arc;

use async_trait::async_trait;
use awayd_core::config::paths;
use awayd_core::config::schema::AppConfig;
use awayd_ipc::SchedulerApi;
use chrono::Datelike;
use tokio::sync::RwLock;

use crate::holidays::{self, load_holidays_or_empty};
use crate::override_state;
use crate::runtime_config::build_config;
use crate::secret;
use crate::state_store::update_state;
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

    async fn has_credential(&self, id: &str) -> bool {
        match secret::get_credential(id).await {
            Ok(Some(t)) => !t.trim().is_empty(),
            _ => false,
        }
    }

    async fn list_subdivisions(&self, country: &str) -> Vec<String> {
        holidays::catalogue::subdivisions_for_country(country)
    }

    async fn list_available_countries(&self) -> Vec<(String, String)> {
        holidays::sync::list_available_countries_cached()
            .await
            .unwrap_or_default()
    }

    async fn holiday_data_status(&self, country: &str) -> (bool, i32, i32, String, String) {
        let s = holidays::sync::holiday_data_status(country);
        (s.ready, s.year_min, s.year_max, s.fetched_at, s.message)
    }

    async fn refresh_holiday_data(&self, country: &str) -> String {
        let cc = country.trim().to_uppercase();
        match holidays::sync::refresh_country(&cc).await {
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
        let prev_country = awayd_core::config::load_config(&path)
            .map(|c| c.schedule.country.to_uppercase())
            .unwrap_or_default();
        let app: AppConfig = match toml::from_str(toml) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(target: "awayd::api", "SetConfigToml parse: {e}");
                return false;
            }
        };
        if let Err(e) = crate::runtime_config::validate_app_config(&app) {
            tracing::warn!(target: "awayd::api", "SetConfigToml validate: {e}");
            return false;
        }
        if let Err(e) = awayd_core::atomic_fs::atomic_write(&path, toml.as_bytes()) {
            tracing::warn!(target: "awayd::api", "SetConfigToml write: {e}");
            return false;
        }
        *self.config_toml.write().await = toml.to_string();
        awayd_core::logging::apply_capture_level(&app.advanced);
        let new_country = app.schedule.country.to_uppercase();
        if new_country != prev_country {
            std::thread::spawn(move || {
                if let Err(e) = holidays::sync::ensure_country_sync(&new_country) {
                    tracing::warn!(
                        target: "awayd::holidays",
                        country = %new_country,
                        error = %e,
                        "holiday sync after country change failed"
                    );
                }
            });
        }
        // Not `poke_immediate`: each settings save (every vacation or hour change) would
        // otherwise re-send the status to every service even when nothing changes.
        self.poke.poke();
        true
    }

    async fn reload_config(&self) -> bool {
        let path = paths::config_file();
        let app = match awayd_core::config::load_config(&path) {
            Ok(app) => app,
            Err(e) => {
                tracing::warn!(target: "awayd::api", "ReloadConfig parse: {e}");
                return false;
            }
        };
        awayd_core::logging::apply_capture_level(&app.advanced);
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(target: "awayd::api", "ReloadConfig read: {e}");
                return false;
            }
        };
        *self.config_toml.write().await = raw;
        self.poke.poke_immediate();
        true
    }

    async fn set_credential(&self, id: &str, value: &str) -> bool {
        if value.trim().is_empty() {
            return false;
        }
        let norm = crate::apply::normalize_credential(id, value);
        // `secret` rejects ids without a credential (unknown backends).
        match secret::set_credential(id, &norm).await {
            Ok(()) => {
                self.poke.poke_immediate();
                true
            }
            Err(e) => {
                tracing::warn!(target: "awayd::api", credential = id, "SetCredential: {e}");
                false
            }
        }
    }

    async fn clear_credential(&self, id: &str) -> bool {
        match secret::clear_credential(id).await {
            Ok(()) => {
                self.poke.poke_immediate();
                true
            }
            Err(e) => {
                tracing::warn!(target: "awayd::api", credential = id, "ClearCredential: {e}");
                false
            }
        }
    }

    async fn apply_now(&self) -> bool {
        tracing::info!(target: "awayd::api", "ApplyNow requested");
        self.poke.poke_immediate();
        true
    }

    async fn set_override(&self, mode: &str, ttl_seconds: u32) -> bool {
        let work_mode = match mode.to_ascii_lowercase().as_str() {
            "invisible" => true,
            "online" => false,
            _ => {
                tracing::warn!(target: "awayd::api", "SetOverride bad mode {mode:?}");
                return false;
            }
        };
        let cfg_path = paths::config_file();
        let app = match awayd_core::config::load_config(&cfg_path) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(target: "awayd::api", "SetOverride load config: {e}");
                return false;
            }
        };
        let work = match build_config(&app) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(target: "awayd::api", "SetOverride build config: {e}");
                return false;
            }
        };
        let now = chrono::Local::now();
        // Missing holiday data must not block a manual override;
        // only the boundary-mode expiry may then ignore a holiday.
        let (hol, hol_warning) =
            load_holidays_or_empty(&app.schedule.country, &app.schedule.subdiv, now.year());
        if let Some(w) = hol_warning {
            tracing::warn!(target: "awayd::api", "SetOverride holidays: {w}");
        }
        let exp = override_state::override_expires_at(now, ttl_seconds, &work, &hol);
        let exp_s = exp.format("%Y-%m-%dT%H:%M:%S").to_string();
        let record = awayd_core::config::state::OverrideRecord {
            version: override_state::OVERRIDE_JSON_VERSION,
            work_mode,
            expires_at: exp_s,
        };
        if let Err(e) = update_state(&paths::state_file(), |st| st.r#override = Some(record)) {
            tracing::warn!(target: "awayd::api", "SetOverride update state: {e:#}");
            return false;
        }
        self.poke.poke_immediate();
        true
    }

    async fn clear_override(&self) -> bool {
        if let Err(e) = update_state(&paths::state_file(), |st| st.r#override = None) {
            tracing::warn!(target: "awayd::api", "ClearOverride update state: {e:#}");
            return false;
        }
        self.poke.poke_immediate();
        true
    }

    fn quit(&self) {
        tracing::info!(target: "awayd::api", "Quit requested");
        self.shutdown.notify_waiters();
    }
}
