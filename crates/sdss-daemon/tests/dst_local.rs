//! SPDX-License-Identifier: GPL-3.0-or-later
//! Public scheduling entry points with `chrono::Local` inside the
//! European autumn fold (2027-10-31 02:30:17 occurs twice).
//!
//! `chrono::Local` follows the Windows time zone and ignores `TZ`, so these calls only hit
//! the fold on a machine set to a European zone; elsewhere they run on an ordinary time.
//! Zone-independent coverage lives in `local_time.rs` and `schedule_next.rs` (chrono-tz).

use chrono::{Local, NaiveDate, TimeZone, Timelike};
use sdss_core::config::state::AppState;
use sdss_core::schedule::default_week_days;
use sdss_daemon::holidays::HolidayLookup;
use sdss_daemon::next_schedule_flip_after_default;
use sdss_daemon::override_state::{effective_work_mode, override_expires_at};
use sdss_daemon::work_window::{Config, ScheduleConfig};

struct NoHolidays;

impl HolidayLookup for NoHolidays {
    fn is_holiday(&self, _d: NaiveDate) -> bool {
        false
    }
}

fn cfg() -> Config {
    Config {
        discord_auth_token: String::new(),
        schedule: ScheduleConfig {
            country: "DE".into(),
            subdiv: "BW".into(),
            days: default_week_days(),
            vacation_ranges: vec![],
        },
    }
}

#[test]
fn scheduling_does_not_panic_in_autumn_fold() {
    let naive = NaiveDate::from_ymd_opt(2027, 10, 31)
        .unwrap()
        .and_hms_opt(2, 30, 17)
        .unwrap();
    let r = Local.from_local_datetime(&naive);
    let instants = [r.earliest().unwrap(), r.latest().unwrap()];
    let cfg = cfg();
    let st = AppState::default();

    for now in instants {
        let (work_mode, source, _) = effective_work_mode(&cfg, now, &NoHolidays, &st);
        assert!(!work_mode, "Sunday night is outside the default work window");
        assert_eq!(source, "schedule");

        let next = next_schedule_flip_after_default(&cfg, now, &NoHolidays).expect("next flip");
        assert!(next > now);
        assert_eq!(next.second(), 0);

        let exp = override_expires_at(now, 3600, &cfg, &NoHolidays);
        assert!(exp > now);
        assert_eq!(exp.second(), 0);
    }
}
