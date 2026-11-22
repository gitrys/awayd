//! SPDX-License-Identifier: GPL-3.0-or-later
//! Missing holiday data must not stop scheduling.
//!
//! One test function on purpose: it sets the process-wide APPDATA, and every file in
//! `tests/` is its own process.

use awayd_daemon::holidays::{load_holiday_nearby_years, load_holidays_or_empty, HolidayLookup};
use chrono::NaiveDate;

#[test]
fn missing_holiday_data_yields_empty_set_and_warning() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPDATA", dir.path());
    std::env::remove_var("AWAYD_PORTABLE");

    // Precondition: no cached, system or embedded data for this country.
    let err = load_holiday_nearby_years("DE", "BW", 2026).expect_err("no data expected");
    assert!(err.contains("no holiday data"), "error: {err}");

    let (set, warning) = load_holidays_or_empty("DE", "BW", 2026);
    let warning = warning.expect("warning for the status");
    assert!(
        warning.contains("no holiday data for country DE"),
        "warning: {warning}"
    );
    assert!(warning.contains("ignored"), "warning: {warning}");
    // New Year's Day is not known without data: scheduling treats it as a normal day.
    assert!(!set.is_holiday(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()));
}
