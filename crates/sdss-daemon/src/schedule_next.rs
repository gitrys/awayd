//! SPDX-License-Identifier: GPL-3.0-or-later
//! Next schedule flip (candidate-boundary algorithm).

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike};

use crate::holidays::HolidayLookup;
use crate::work_window::{self, Config, ScheduleConfig};

const DEFAULT_MAX_DAYS: i64 = 14;

fn date_blocked(
    d: NaiveDate,
    sched: &ScheduleConfig,
    de: &dyn HolidayLookup,
) -> bool {
    work_window::date_in_vacation_ranges(d, &sched.vacation_ranges) || de.is_holiday(d)
}

fn flip_candidates(
    cfg: &ScheduleConfig,
    start_date: NaiveDate,
    max_days: i64,
    de: &dyn HolidayLookup,
) -> Vec<DateTime<Local>> {
    let mut candidates = Vec::new();
    for day_offset in 0..=max_days {
        let d = start_date + Duration::days(day_offset);
        let iso = d.weekday().number_from_monday() as usize;
        let day = &cfg.days[iso - 1];

        if day.enabled && !date_blocked(d, cfg, de) {
            candidates.push(Local.from_local_datetime(&d.and_time(day.start)).unwrap());
            let end_plus = d.and_time(day.end) + Duration::minutes(1);
            candidates.push(Local.from_local_datetime(&end_plus).unwrap());
        }

        candidates.push(
            Local
                .from_local_datetime(
                    &(d + Duration::days(1)).and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
                )
                .unwrap(),
        );
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Smallest `t > now` where `want_work_mode` changes vs `now`.
pub fn next_schedule_flip_after(
    cfg: &Config,
    now: DateTime<Local>,
    de: &dyn HolidayLookup,
    max_days: i64,
) -> Result<DateTime<Local>, String> {
    let now = now.with_second(0).unwrap().with_nanosecond(0).unwrap();
    let baseline = work_window::want_work_mode(cfg, now, de);
    let horizon = now + Duration::days(max_days);
    let sched = &cfg.schedule;

    for t in flip_candidates(sched, now.date_naive(), max_days, de) {
        let t = t.with_second(0).unwrap().with_nanosecond(0).unwrap();
        if t <= now {
            continue;
        }
        if t > horizon {
            break;
        }
        if work_window::want_work_mode(cfg, t, de) != baseline {
            return Ok(t);
        }
    }
    Err(format!(
        "No schedule mode change within {max_days} days after {}",
        now.format("%Y-%m-%d %H:%M:%S")
    ))
}

pub fn next_schedule_flip_after_default(
    cfg: &Config,
    now: DateTime<Local>,
    de: &dyn HolidayLookup,
) -> Result<DateTime<Local>, String> {
    next_schedule_flip_after(cfg, now, de, DEFAULT_MAX_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use sdss_core::config::schema::DayWindow;
    use sdss_core::schedule::default_week_days;

    struct NoHolidays;

    impl HolidayLookup for NoHolidays {
        fn is_holiday(&self, _d: chrono::NaiveDate) -> bool {
            false
        }
    }

    fn cfg_with_days(mut days: [DayWindow; 7]) -> Config {
        Config {
            discord_auth_token: String::new(),
            schedule: ScheduleConfig {
                country: "DE".into(),
                subdiv: "BW".into(),
                days,
                vacation_ranges: vec![],
            },
        }
    }

    #[test]
    fn next_flip_respects_fri_short_day() {
        let mut days = default_week_days();
        days[4].end = chrono::NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let cfg = cfg_with_days(days);
        let holi = NoHolidays;

        let fri_1300 = NaiveDate::from_ymd_opt(2026, 6, 5)
            .unwrap()
            .and_hms_opt(13, 0, 0)
            .unwrap();
        let now = Local.from_local_datetime(&fri_1300).unwrap();
        let next = next_schedule_flip_after_default(&cfg, now, &holi).unwrap();
        assert_eq!(next.hour(), 14);
        assert_eq!(next.minute(), 1);
    }
}
