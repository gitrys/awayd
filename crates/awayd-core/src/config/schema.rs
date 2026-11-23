//! SPDX-License-Identifier: GPL-3.0-or-later
//! `config.toml` serde schema (versioned).

use std::collections::BTreeMap;

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    /// `[backends.<id>]` per service (`awayd_core::backends`). A service without entry is
    /// switched on, so configurations from 0.1.0 (no section) keep managing both services.
    /// The empty `[discord]` section of 0.1.0 is ignored when read.
    #[serde(default)]
    pub backends: BTreeMap<String, BackendSettings>,
    #[serde(default)]
    pub schedule: ScheduleSection,
    #[serde(default)]
    pub mode: ModeSection,
    #[serde(default)]
    pub advanced: AdvancedSection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl AppConfig {
    /// Service `id` is switched on (no entry = on).
    pub fn backend_enabled(&self, id: &str) -> bool {
        self.backends.get(id).is_none_or(|b| b.enabled)
    }

    pub fn set_backend_enabled(&mut self, id: &str, enabled: bool) {
        self.backends
            .insert(id.to_string(), BackendSettings { enabled });
    }
}

/// Explicit entry for every service with its default (new configurations).
fn default_backends() -> BTreeMap<String, BackendSettings> {
    crate::backends::ALL
        .iter()
        .map(|b| {
            (
                b.id.to_string(),
                BackendSettings {
                    enabled: b.enabled_by_default,
                },
            )
        })
        .collect()
}

fn default_subdiv() -> String {
    "BW".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DayWindow {
    #[serde(default)]
    pub enabled: bool,
    #[serde(with = "time_hhmm", default = "default_time_start")]
    pub start: NaiveTime,
    #[serde(with = "time_hhmm", default = "default_time_end")]
    pub end: NaiveTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleSection {
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default)]
    pub subdiv: String,
    /// Mon..Sun (ISO 1..7). Absent in legacy configs until `normalize_schedule`.
    #[serde(default)]
    pub days: Option<[DayWindow; 7]>,
    /// Legacy — read on load; omitted on save when `days` is set.
    #[serde(with = "time_hhmm", default = "default_time_start")]
    pub time_start: NaiveTime,
    #[serde(with = "time_hhmm", default = "default_time_end")]
    pub time_end: NaiveTime,
    #[serde(default = "default_weekdays")]
    pub weekdays: String,
    #[serde(default)]
    pub vacations: Vec<VacationEntry>,
}

impl Serialize for ScheduleSection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let legacy = self.days.is_none();
        let field_count = 3 + if legacy { 3 } else { 1 } + 1;
        let mut st = serializer.serialize_struct("ScheduleSection", field_count)?;
        st.serialize_field("country", &self.country)?;
        st.serialize_field("subdiv", &self.subdiv)?;
        if let Some(days) = &self.days {
            st.serialize_field("days", days)?;
        } else {
            st.serialize_field("time_start", &self.time_start)?;
            st.serialize_field("time_end", &self.time_end)?;
            st.serialize_field("weekdays", &self.weekdays)?;
        }
        st.serialize_field("vacations", &self.vacations)?;
        st.end()
    }
}

impl Default for ScheduleSection {
    fn default() -> Self {
        Self {
            country: default_country(),
            subdiv: default_subdiv(),
            days: Some(crate::schedule::default_week_days()),
            time_start: default_time_start(),
            time_end: default_time_end(),
            weekdays: default_weekdays(),
            vacations: vec![],
        }
    }
}

fn default_country() -> String {
    "DE".to_string()
}

fn default_time_start() -> NaiveTime {
    NaiveTime::from_hms_opt(9, 0, 0).unwrap()
}

fn default_time_end() -> NaiveTime {
    NaiveTime::from_hms_opt(17, 0, 0).unwrap()
}

fn default_weekdays() -> String {
    "1-5".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VacationEntry {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSection {
    #[serde(default = "default_mode_kind")]
    pub kind: String,
    /// Network mode: how long an override "until the next change" (`0` / `boundary`) lasts,
    /// since schedule changes do not apply there.
    #[serde(default = "default_network_ttl")]
    pub network_override_ttl_hours: f64,
    /// Hostname or IP for `kind = "network"` (present → work / invisible).
    #[serde(default)]
    pub work_host: String,
    /// TCP port for `probe = "tcp"`.
    #[serde(default = "default_work_port")]
    pub work_port: u16,
    /// How `work_host` is checked: `tcp` (connect to `work_port`) or `ping` (ICMP echo, e.g.
    /// a work PC in the home network whose firewall blocks all ports).
    #[serde(default)]
    pub probe: ProbeKind,
    /// Network mode: seconds between two checks of `work_host` (minimum 10).
    #[serde(default = "default_probe_interval")]
    pub probe_interval_seconds: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeKind {
    #[default]
    Tcp,
    Ping,
}

pub const MIN_PROBE_INTERVAL_SECONDS: u64 = 10;

fn default_probe_interval() -> u64 {
    60
}

fn default_work_port() -> u16 {
    22
}

fn default_mode_kind() -> String {
    "schedule".to_string()
}

fn default_network_ttl() -> f64 {
    8.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedSection {
    #[serde(default = "default_poke_ms")]
    pub poke_debounce_ms: u64,
    #[serde(default = "default_log_max_bytes")]
    pub log_max_bytes: u64,
    #[serde(default = "default_log_max_files")]
    pub log_max_files: usize,
    #[serde(default = "default_log_min_level")]
    pub log_min_level: String,
    /// Seconds between two checks for newly started programs of switched-on services (e.g.
    /// Steam, Discord); `0` turns the check off (the status is then applied only at
    /// scheduled changes and on request).
    #[serde(default = "default_process_watch_seconds")]
    pub process_watch_seconds: u64,
}

/// Allowed `advanced.process_watch_seconds`: 0 (off) or this range.
pub const PROCESS_WATCH_SECONDS_RANGE: std::ops::RangeInclusive<u64> = 2..=60;

fn default_process_watch_seconds() -> u64 {
    5
}

fn default_poke_ms() -> u64 {
    250
}

fn default_log_max_bytes() -> u64 {
    5 * 1024 * 1024
}

fn default_log_max_files() -> usize {
    5
}

fn default_log_min_level() -> String {
    "debug".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            backends: default_backends(),
            schedule: ScheduleSection::default(),
            mode: ModeSection {
                kind: default_mode_kind(),
                network_override_ttl_hours: default_network_ttl(),
                work_host: String::new(),
                work_port: default_work_port(),
                probe: ProbeKind::default(),
                probe_interval_seconds: default_probe_interval(),
            },
            advanced: AdvancedSection {
                poke_debounce_ms: default_poke_ms(),
                log_max_bytes: default_log_max_bytes(),
                log_max_files: default_log_max_files(),
                log_min_level: default_log_min_level(),
                process_watch_seconds: default_process_watch_seconds(),
            },
        }
    }
}

impl Default for ModeSection {
    fn default() -> Self {
        Self {
            kind: default_mode_kind(),
            network_override_ttl_hours: default_network_ttl(),
            work_host: String::new(),
            work_port: default_work_port(),
            probe: ProbeKind::default(),
            probe_interval_seconds: default_probe_interval(),
        }
    }
}

impl Default for AdvancedSection {
    fn default() -> Self {
        Self {
            poke_debounce_ms: default_poke_ms(),
            log_max_bytes: default_log_max_bytes(),
            log_max_files: default_log_max_files(),
            log_min_level: default_log_min_level(),
            process_watch_seconds: default_process_watch_seconds(),
        }
    }
}

mod time_hhmm {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(t: &NaiveTime, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_str(&t.format("%H:%M").to_string())
    }

    pub fn deserialize<'de, D>(d: D) -> Result<NaiveTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        NaiveTime::parse_from_str(&s, "%H:%M").map_err(serde::de::Error::custom)
    }
}
