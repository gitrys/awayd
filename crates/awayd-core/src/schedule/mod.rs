//! SPDX-License-Identifier: GPL-3.0-or-later
//! Per-weekday schedule normalization, validation, and helpers.

use chrono::NaiveTime;

use crate::config::schema::{DayWindow, ScheduleSection};
use crate::schedule_util::parse_weekdays;

pub const DAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub fn default_week_days() -> [DayWindow; 7] {
    let start = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
    let end = NaiveTime::from_hms_opt(17, 0, 0).unwrap();
    std::array::from_fn(|i| DayWindow {
        enabled: i < 5,
        start,
        end,
    })
}

/// Expand legacy `weekdays` + global start/end into `days` when absent from config.
pub fn normalize_schedule(schedule: &mut ScheduleSection) {
    if schedule.days.is_some() {
        return;
    }
    let mut days = default_week_days();
    if let Ok(set) = parse_weekdays(&schedule.weekdays) {
        for (i, day) in days.iter_mut().enumerate() {
            let iso = (i + 1) as u8;
            day.enabled = set.contains(&iso);
            day.start = schedule.time_start;
            day.end = schedule.time_end;
        }
    }
    schedule.days = Some(days);
}

pub fn schedule_days(schedule: &ScheduleSection) -> [DayWindow; 7] {
    schedule.days.unwrap_or_else(default_week_days)
}

pub fn validate_schedule(schedule: &ScheduleSection) -> Result<(), String> {
    let days = schedule
        .days
        .as_ref()
        .ok_or_else(|| "schedule.days missing (call normalize_schedule first)".to_string())?;

    let enabled_count = days.iter().filter(|d| d.enabled).count();
    if enabled_count == 0 {
        return Err("at least one weekday must be enabled".into());
    }

    for (i, day) in days.iter().enumerate() {
        if !day.enabled {
            continue;
        }
        if day.start > day.end {
            return Err(format!(
                "{}: start ({}) must be <= end ({}) (overnight shifts not supported yet)",
                DAY_LABELS[i],
                day.start.format("%H:%M"),
                day.end.format("%H:%M")
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::ScheduleSection;

    #[test]
    fn migrate_legacy_mon_fri() {
        let mut s = ScheduleSection {
            days: None,
            weekdays: "1-5".into(),
            ..ScheduleSection::default()
        };
        normalize_schedule(&mut s);
        let days = s.days.as_ref().unwrap();
        assert!(days[0].enabled);
        assert!(days[4].enabled);
        assert!(!days[5].enabled);
        assert!(!days[6].enabled);
        assert_eq!(days[0].start.format("%H:%M").to_string(), "09:00");
    }

    #[test]
    fn reject_all_disabled() {
        let mut days = default_week_days();
        for d in &mut days {
            d.enabled = false;
        }
        let s = ScheduleSection {
            days: Some(days),
            ..ScheduleSection::default()
        };
        assert!(validate_schedule(&s).is_err());
    }

    #[test]
    fn reject_overnight_row() {
        let mut days = default_week_days();
        days[0].enabled = true;
        days[0].start = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        days[0].end = NaiveTime::from_hms_opt(4, 0, 0).unwrap();
        let s = ScheduleSection {
            days: Some(days),
            ..ScheduleSection::default()
        };
        assert!(validate_schedule(&s).is_err());
    }

    #[test]
    fn legacy_toml_migrates_on_load() {
        let raw = r#"
schema_version = 1
[schedule]
country = "DE"
subdiv = "BW"
time_start = "08:00"
time_end = "16:00"
weekdays = "1,2,3,4,5"
[mode]
kind = "schedule"
"#;
        let mut cfg: crate::config::schema::AppConfig = toml::from_str(raw).unwrap();
        normalize_schedule(&mut cfg.schedule);
        let days = cfg.schedule.days.unwrap();
        assert!(days[0].enabled);
        assert_eq!(days[0].start.format("%H:%M").to_string(), "08:00");
        assert_eq!(days[0].end.format("%H:%M").to_string(), "16:00");
    }
}
