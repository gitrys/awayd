//! SPDX-License-Identifier: GPL-3.0-or-later

use awayd_core::schedule::default_week_days;
use awayd_daemon::holidays::HolidayLookup;
use awayd_daemon::override_state::{override_expires_at, OVERRIDE_BOUNDARY_FALLBACK_SECS};
use awayd_daemon::work_window::{Config, ScheduleConfig};
use chrono::{Local, NaiveDate, TimeZone, Timelike};

struct FixedHoliday(NaiveDate);

impl HolidayLookup for FixedHoliday {
    fn is_holiday(&self, d: NaiveDate) -> bool {
        d == self.0
    }
}

fn weekday_sched() -> Config {
    Config {
        schedule: ScheduleConfig {
            country: "DE".into(),
            subdiv: "BW".into(),
            days: default_week_days(),
            vacation_ranges: vec![],
        },
    }
}

fn local_at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(y, m, d, h, mi, 0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}

#[test]
fn boundary_mode_expires_at_end_of_work_window() {
    let cfg = weekday_sched();
    let hol = FixedHoliday(NaiveDate::from_ymd_opt(2099, 1, 1).unwrap());
    let now = local_at(2026, 5, 12, 10, 0); // Tuesday
    let exp = override_expires_at(now, 0, &cfg, &hol);
    assert_eq!(exp.hour(), 17);
    assert_eq!(exp.minute(), 1);
    assert_eq!(exp.date_naive(), now.date_naive());
}

#[test]
fn boundary_mode_caps_at_eight_hours() {
    let cfg = weekday_sched();
    let hol = FixedHoliday(NaiveDate::from_ymd_opt(2099, 1, 1).unwrap());
    // Saturday: next schedule flip is Monday 09:00 (> 8h); cap applies.
    let now = local_at(2026, 5, 16, 12, 0);
    let exp = override_expires_at(now, 0, &cfg, &hol);
    let cap = now + chrono::Duration::seconds(i64::from(OVERRIDE_BOUNDARY_FALLBACK_SECS));
    assert_eq!(exp, cap);
}

#[test]
fn fixed_ttl_ignores_boundary() {
    let cfg = weekday_sched();
    let hol = FixedHoliday(NaiveDate::from_ymd_opt(2099, 1, 1).unwrap());
    let now = local_at(2026, 5, 12, 10, 0);
    let exp = override_expires_at(now, 7200, &cfg, &hol);
    let want = now + chrono::Duration::seconds(7200);
    assert_eq!(exp, want);
}
