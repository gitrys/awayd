//! SPDX-License-Identifier: GPL-3.0-or-later
//! Holiday file parse + load from fixture (schema v2).

use awayd_daemon::holidays::data::load_holiday_set;

const DE_FIXTURE: &str = include_str!("fixtures/de_v2_sample.json");

#[test]
fn fixture_v2_metadata() {
    let v: serde_json::Value = serde_json::from_str(DE_FIXTURE).unwrap();
    assert_eq!(v["schema_version"], 2);
    assert_eq!(v["year_min"], 2026);
}

fn write_holiday_fixture(dir: &tempfile::TempDir) {
    std::env::remove_var("AWAYD_PORTABLE");
    std::env::set_var("APPDATA", dir.path());
    let holidays_dir = awayd_core::config::paths::holidays_dir();
    std::fs::create_dir_all(&holidays_dir).unwrap();
    std::fs::write(holidays_dir.join("DE.json"), DE_FIXTURE).unwrap();
}

#[test]
fn fixture_v2_loads_subdiv() {
    let dir = tempfile::tempdir().unwrap();
    write_holiday_fixture(&dir);

    let s = load_holiday_set("DE", "BY", 2026).expect("load");
    assert!(s.is_holiday(chrono::NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()));
}

#[test]
fn fixture_v2_list_holidays_date_only_fallback() {
    use awayd_daemon::holidays::data::list_holidays_for_subdiv;

    let dir = tempfile::tempdir().unwrap();
    write_holiday_fixture(&dir);

    let list = list_holidays_for_subdiv("DE", "BY", 2026).expect("list");
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|e| !e.has_proper_name()));
    assert_eq!(list[0].date, "2026-01-01");
}

#[test]
fn fixture_v2_year_summary() {
    use awayd_daemon::holidays::data::holiday_year_summary_on;

    let dir = tempfile::tempdir().unwrap();
    write_holiday_fixture(&dir);

    let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let s = holiday_year_summary_on("DE", "BY", 2026, today).expect("summary");
    assert_eq!(s.total, 2);
    assert_eq!(s.remaining, 2);
}
