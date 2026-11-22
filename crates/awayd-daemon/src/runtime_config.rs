//! SPDX-License-Identifier: GPL-3.0-or-later
//! Build `work_window::Config` from persisted `AppConfig` + Discord token.

use chrono::NaiveDate;

use crate::work_window::{Config, ScheduleConfig};
use awayd_core::config::schema::{AppConfig, VacationEntry};
use awayd_core::schedule::{normalize_schedule, schedule_days, validate_schedule};

pub fn vacation_ranges_from_entries(
    entries: &[VacationEntry],
) -> Result<Vec<(NaiveDate, NaiveDate)>, String> {
    let mut out = Vec::new();
    for e in entries {
        let start =
            NaiveDate::parse_from_str(&e.start, "%Y-%m-%d").map_err(|err| err.to_string())?;
        let end = NaiveDate::parse_from_str(&e.end, "%Y-%m-%d").map_err(|err| err.to_string())?;
        if start > end {
            return Err(format!("vacation {}: start after end", e.id));
        }
        out.push((start, end));
    }
    Ok(out)
}

/// Reject invalid schedule fields before persisting or applying.
pub fn validate_app_config(app: &AppConfig) -> Result<(), String> {
    let mut schedule = app.schedule.clone();
    normalize_schedule(&mut schedule);
    validate_schedule(&schedule)?;
    let _ = vacation_ranges_from_entries(&app.schedule.vacations)?;
    Ok(())
}

pub fn build_config(app: &AppConfig, discord_token: &str) -> Result<Config, String> {
    let mut schedule = app.schedule.clone();
    normalize_schedule(&mut schedule);
    validate_app_config(app)?;
    let vacation_ranges = vacation_ranges_from_entries(&app.schedule.vacations)?;
    Ok(Config {
        discord_auth_token: discord_token.to_string(),
        schedule: ScheduleConfig {
            country: schedule.country.clone(),
            subdiv: schedule.subdiv.clone(),
            days: schedule_days(&schedule),
            vacation_ranges,
        },
    })
}
