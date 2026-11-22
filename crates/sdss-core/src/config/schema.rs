//! SPDX-License-Identifier: GPL-3.0-or-later
//! `config.toml` serde schema (versioned).

use chrono::NaiveTime;
use serde::{Deserialize, Serialize};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub discord: DiscordSection,
    #[serde(default)]
    pub schedule: ScheduleSection,
    #[serde(default)]
    pub mode: ModeSection,
    #[serde(default)]
    pub advanced: AdvancedSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordSection {}

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
    #[serde(default = "default_network_ttl")]
    pub network_override_ttl_hours: f64,
    /// Hostname or IP for `kind = "network"` (reachable → invisible).
    #[serde(default)]
    pub work_host: String,
    #[serde(default = "default_work_port")]
    pub work_port: u16,
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
            discord: DiscordSection::default(),
            schedule: ScheduleSection::default(),
            mode: ModeSection {
                kind: default_mode_kind(),
                network_override_ttl_hours: default_network_ttl(),
                work_host: String::new(),
                work_port: default_work_port(),
            },
            advanced: AdvancedSection {
                poke_debounce_ms: default_poke_ms(),
                log_max_bytes: default_log_max_bytes(),
                log_max_files: default_log_max_files(),
                log_min_level: default_log_min_level(),
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
