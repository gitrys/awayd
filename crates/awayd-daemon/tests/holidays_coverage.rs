//! SPDX-License-Identifier: GPL-3.0-or-later
//! A year missing from the holiday file must be reported instead of silently yielding no
//! holidays (the coverage warnings existed but were never called).
//!
//! One test function on purpose: it sets the process-wide APPDATA, and every file in
//! `tests/` is its own process.

use awayd_daemon::holidays::{load_holidays_or_empty, HolidayLookup};
use chrono::NaiveDate;

const DE_FIXTURE: &str = include_str!("fixtures/de_v2_sample.json"); // covers 2026 only

#[test]
fn missing_year_is_reported_but_scheduling_continues() {
    let dir = tempfile::tempdir().unwrap();
    std::env::remove_var("AWAYD_PORTABLE");
    std::env::set_var("APPDATA", dir.path());
    let holidays_dir = awayd_core::config::paths::holidays_dir();
    std::fs::create_dir_all(&holidays_dir).unwrap();
    std::fs::write(holidays_dir.join("DE.json"), DE_FIXTURE).unwrap();

    // 2026 is covered, but the schedule also looks into 2027.
    let (set, warning) = load_holidays_or_empty("DE", "BY", 2026);
    assert!(set.is_holiday(NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()));
    let warning = warning.expect("next year missing must be reported");
    assert!(warning.contains("does not cover 2027"), "{warning}");
    assert!(!warning.contains("does not cover 2026"), "{warning}");

    // 2030 is not covered at all: empty set (tick continues) plus a warning.
    let (set, warning) = load_holidays_or_empty("DE", "BY", 2030);
    assert!(!set.is_holiday(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()));
    let warning = warning.expect("missing year must be reported");
    assert!(warning.contains("does not cover 2030"), "{warning}");
    assert!(warning.contains("ignored"), "{warning}");
}
