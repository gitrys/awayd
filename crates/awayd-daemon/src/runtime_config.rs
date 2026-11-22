//! SPDX-License-Identifier: GPL-3.0-or-later
//! Build `work_window::Config` from persisted `AppConfig` + Discord token.

use crate::work_window::{Config, ScheduleConfig};
use awayd_core::config::schema::AppConfig;
use awayd_core::schedule::{normalize_schedule, schedule_days};

// Shared with `awayctl validate` and the settings window (one validation for all writers).
pub use awayd_core::config::{vacation_ranges_from_entries, validate_app_config};

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
