//! SPDX-License-Identifier: GPL-3.0-or-later
//! Nager.Date fetch + cache integration (wiremock).

use sdss_daemon::holidays::data::load_holiday_set;
use sdss_daemon::holidays::sync;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn ensure_country_writes_cache_and_loads() {
    let rt = tokio::runtime::Runtime::new().expect("tokio rt");
    let server = rt.block_on(MockServer::start());
    unsafe {
        std::env::set_var("SDSS_NAGER_API_BASE", server.uri());
    }

    let sample = r#"[
      {
        "date": "2026-01-01",
        "localName": "Neujahr",
        "name": "New Year's Day",
        "countryCode": "DE",
        "global": true,
        "counties": null,
        "types": ["Public"]
      },
      {
        "date": "2026-08-15",
        "localName": "Mariä Himmelfahrt",
        "name": "Assumption Day",
        "countryCode": "DE",
        "global": false,
        "counties": ["DE-BY"],
        "types": ["Public"]
      }
    ]"#;

    rt.block_on(async {
        for y in 2025..=2029 {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/PublicHolidays/{y}/DE")))
                .respond_with(ResponseTemplate::new(200).set_body_string(sample))
                .mount(&server)
                .await;
        }
    });

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPDATA", dir.path());
    let cache_path = sync::cache_path("DE");

    sync::ensure_country_sync("DE").expect("ensure DE");

    assert!(cache_path.is_file());
    let raw = std::fs::read(&cache_path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(v["schema_version"], 3);
    assert_eq!(v["source"].as_str(), Some("nager.date"));
    assert!(v["year_min"].as_i64().unwrap() <= 2025);
    assert!(v["year_max"].as_i64().unwrap() >= 2028);

    let set = load_holiday_set("DE", "BY", 2026).expect("load BY");
    assert!(set.is_holiday(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()));
    assert!(set.is_holiday(chrono::NaiveDate::from_ymd_opt(2026, 8, 15).unwrap()));

    let by_holidays = &v["by_year"]["2026"]["by_subdiv"]["BY"]["by_category"]["public"];
    let first = &by_holidays[0];
    assert!(first.get("local_name").is_some() || first.get("localName").is_some() || first.is_string());
    assert!(first.get("name").is_some() || first.is_string());
}
