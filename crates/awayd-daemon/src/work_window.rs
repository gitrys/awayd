//! SPDX-License-Identifier: GPL-3.0-or-later
//! Work window: per-weekday hours, holidays lookup, local time range.

use awayd_core::config::schema::DayWindow;
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Timelike};

use crate::holidays::HolidayLookup;

#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    pub country: String,
    pub subdiv: String,
    pub days: [DayWindow; 7],
    pub vacation_ranges: Vec<(NaiveDate, NaiveDate)>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub discord_auth_token: String,
    pub schedule: ScheduleConfig,
}

pub fn schedule_wants_work_mode(
    sched: &ScheduleConfig,
    when: DateTime<chrono::Local>,
    holidays: &dyn HolidayLookup,
) -> bool {
    is_work_window(when, sched, holidays)
}

pub fn want_work_mode(
    cfg: &Config,
    when: DateTime<chrono::Local>,
    holidays: &dyn HolidayLookup,
) -> bool {
    schedule_wants_work_mode(&cfg.schedule, when, holidays)
}

pub fn parse_hhmm(s: &str) -> Result<NaiveTime, String> {
    awayd_core::schedule_util::parse_hhmm(s)
}

fn time_to_minutes(t: NaiveTime) -> u32 {
    t.hour() * 60 + t.minute()
}

pub fn date_in_vacation_ranges(d: NaiveDate, ranges: &[(NaiveDate, NaiveDate)]) -> bool {
    for (start, end) in ranges {
        if (*start..=*end).contains(&d) {
            return true;
        }
    }
    false
}

pub fn is_work_window(
    when: DateTime<chrono::Local>,
    sched: &ScheduleConfig,
    de_holidays: &dyn HolidayLookup,
) -> bool {
    let d = when.date_naive();
    if date_in_vacation_ranges(d, &sched.vacation_ranges) {
        return false;
    }
    if de_holidays.is_holiday(d) {
        return false;
    }

    let iso = when.weekday().number_from_monday() as usize;
    let day = &sched.days[iso - 1];
    if !day.enabled {
        return false;
    }

    let now_m = time_to_minutes(
        when.time()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap(),
    );
    let start_m = time_to_minutes(day.start);
    let end_m = time_to_minutes(day.end);
    (start_m..=end_m).contains(&now_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use awayd_core::schedule::default_week_days;
    use chrono::{Local, TimeZone};

    struct NoHolidays;

    impl HolidayLookup for NoHolidays {
        fn is_holiday(&self, _d: NaiveDate) -> bool {
            false
        }
    }

    fn sched_with_days(mut days: [DayWindow; 7]) -> ScheduleConfig {
        ScheduleConfig {
            country: "DE".into(),
            subdiv: "BW".into(),
            days,
            vacation_ranges: vec![],
        }
    }

    #[test]
    fn fri_short_day() {
        let mut days = default_week_days();
        days[4].end = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let sched = sched_with_days(days);
        let holi = NoHolidays;

        let fri_noon = NaiveDate::from_ymd_opt(2026, 6, 5)
            .unwrap()
            .and_hms_opt(13, 0, 0)
            .unwrap();
        let when = Local.from_local_datetime(&fri_noon).unwrap();
        assert!(is_work_window(when, &sched, &holi));

        let fri_afternoon = NaiveDate::from_ymd_opt(2026, 6, 5)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap();
        let when = Local.from_local_datetime(&fri_afternoon).unwrap();
        assert!(!is_work_window(when, &sched, &holi));
    }
}
