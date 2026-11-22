//! SPDX-License-Identifier: GPL-3.0-or-later
//! Manual override from `state.json` + effective work mode.

use chrono::{DateTime, Duration, Local, NaiveDateTime, Timelike};

use crate::holidays::HolidayLookup;
use crate::local_time::trim_to_minute;
use crate::schedule_next::next_schedule_flip_after_default;
use crate::work_window::{self, Config};
use sdss_core::config::state::{AppState, OverrideRecord};

pub const OVERRIDE_JSON_VERSION: u32 = 1;

/// Max manual override when using boundary mode (`ttl_seconds == 0`) or when no flip is found.
pub const OVERRIDE_BOUNDARY_FALLBACK_SECS: u32 = 8 * 3600;

/// Compute `expires_at` for `SetOverride`.
///
/// - `ttl_seconds > 0`: fixed duration from `now`.
/// - `ttl_seconds == 0`: next schedule boundary, capped at [`OVERRIDE_BOUNDARY_FALLBACK_SECS`];
///   if no boundary within the search horizon, use the cap.
pub fn override_expires_at(
    now: DateTime<Local>,
    ttl_seconds: u32,
    cfg: &Config,
    hol: &dyn HolidayLookup,
) -> DateTime<Local> {
    let now = trim_to_minute(now);
    if ttl_seconds > 0 {
        return trim_to_minute(now + Duration::seconds(i64::from(ttl_seconds)));
    }
    let cap = trim_to_minute(now + Duration::seconds(i64::from(OVERRIDE_BOUNDARY_FALLBACK_SECS)));
    match next_schedule_flip_after_default(cfg, now, hol) {
        Ok(boundary) if boundary <= cap => trim_to_minute(boundary),
        Ok(_) => cap,
        Err(_) => cap,
    }
}

#[derive(Debug, Clone)]
pub struct ManualOverride {
    pub version: u32,
    pub work_mode: bool,
    pub expires_at: NaiveDateTime,
}

impl TryFrom<&OverrideRecord> for ManualOverride {
    type Error = String;

    fn try_from(r: &OverrideRecord) -> Result<Self, Self::Error> {
        if r.version != OVERRIDE_JSON_VERSION {
            return Err(format!("unsupported override version {}", r.version));
        }
        let mut exp = NaiveDateTime::parse_from_str(&r.expires_at, "%Y-%m-%dT%H:%M:%S")
            .map_err(|e| e.to_string())?;
        exp = exp.with_second(0).unwrap().with_nanosecond(0).unwrap();
        Ok(ManualOverride {
            version: r.version,
            work_mode: r.work_mode,
            expires_at: exp,
        })
    }
}

pub fn override_from_state(st: &AppState) -> Result<Option<ManualOverride>, String> {
    let Some(rec) = &st.r#override else {
        return Ok(None);
    };
    ManualOverride::try_from(rec).map(Some)
}

/// `source` is `"manual_override"` or `"schedule"`.
pub fn effective_work_mode(
    cfg: &Config,
    now: DateTime<Local>,
    de: &dyn HolidayLookup,
    st: &AppState,
) -> (bool, &'static str, Option<ManualOverride>) {
    let now = trim_to_minute(now);
    let now_naive = now.naive_local();

    let ov = match override_from_state(st) {
        Ok(o) => o,
        Err(_) => {
            return (work_window::want_work_mode(cfg, now, de), "schedule", None);
        }
    };

    let Some(rec) = ov else {
        return (work_window::want_work_mode(cfg, now, de), "schedule", None);
    };
    if now_naive >= rec.expires_at {
        return (work_window::want_work_mode(cfg, now, de), "schedule", None);
    }
    (rec.work_mode, "manual_override", Some(rec))
}
