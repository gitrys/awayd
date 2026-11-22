//! SPDX-License-Identifier: GPL-3.0-or-later

use chrono::{Local, NaiveDate, TimeZone};
use sdss_core::schedule::default_week_days;
use sdss_daemon::holidays::HolidayLookup;
use sdss_daemon::work_window::{schedule_wants_work_mode, ScheduleConfig};

struct FixedHoliday(NaiveDate);

impl HolidayLookup for FixedHoliday {
    fn is_holiday(&self, d: NaiveDate) -> bool {
        d == self.0
    }
}

#[test]
fn work_window_skips_public_holiday() {
    let holi = FixedHoliday(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let sched = ScheduleConfig {
        country: "DE".into(),
        subdiv: "BW".into(),
        days: default_week_days(),
        vacation_ranges: vec![],
    };
    let mid = NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();
    let when = Local.from_local_datetime(&mid).unwrap();
    assert!(!schedule_wants_work_mode(&sched, when, &holi));
}
