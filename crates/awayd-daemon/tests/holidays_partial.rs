//! SPDX-License-Identifier: GPL-3.0-or-later
//! Holiday sync with a missing year: an optional year must not discard the others (it is
//! taken from the previous file), a missing required year (this or next) keeps the old file.
//!
//! One test function on purpose: it sets process-wide env vars (APPDATA, Nager base URL),
//! and every file in `tests/` is its own process.

use awayd_daemon::holidays::sync;
use chrono::{Datelike, Utc};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample(year: i32) -> String {
    format!(
        r#"[{{"date":"{year}-12-25","localName":"1. Weihnachtstag","name":"Christmas Day","countryCode":"DE","global":true,"counties":null,"types":["Public"]}}]"#
    )
}

/// Previous cache file that has `year` with a marker holiday.
fn previous_file(year: i32) -> String {
    format!(
        r#"{{"schema_version":3,"country":"DE","source":"nager.date","fetched_at":"2026-01-01T00:00:00+00:00","year_min":{year},"year_max":{year},"by_year":{{"{year}":{{"by_subdiv":{{"":{{"by_category":{{"public":[{{"date":"{year}-05-01","name":"Old marker","local_name":null}}]}}}}}}}}}}}}"#
    )
}

#[test]
fn optional_year_missing_keeps_others_required_year_missing_keeps_file() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPDATA", dir.path());
    std::env::remove_var("AWAYD_PORTABLE");

    let (y0, y1) = sync::year_window(Utc::now());
    let this_year = Utc::now().year();
    let last = y1; // optional (this year + 3)
    let cache = sync::cache_path("DE");
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(&cache, previous_file(last)).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();

    // 1) Optional year answers 404: the file is written with the other years, the missing
    //    one comes from the previous file.
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        for y in y0..=y1 {
            let response = if y == last {
                ResponseTemplate::new(404)
            } else {
                ResponseTemplate::new(200).set_body_string(sample(y))
            };
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/PublicHolidays/{y}/DE")))
                .respond_with(response)
                .mount(&server)
                .await;
        }
    });
    std::env::set_var("AWAYD_NAGER_API_BASE", server.uri());
    sync::refresh_country_sync("DE").expect("optional year missing must not fail the sync");

    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cache).unwrap()).unwrap();
    let by_year = v["by_year"].as_object().unwrap();
    for y in y0..y1 {
        assert!(by_year.contains_key(&y.to_string()), "year {y} missing");
    }
    let kept = serde_json::to_string(&by_year[&last.to_string()]).unwrap();
    assert!(
        kept.contains("Old marker"),
        "previous {last} not kept: {kept}"
    );
    assert_eq!(v["year_min"], y0);
    assert_eq!(v["year_max"], y1);

    // 2) The current year answers 404: error, the file stays as it is.
    let before = std::fs::read_to_string(&cache).unwrap();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        for y in y0..=y1 {
            let response = if y == this_year {
                ResponseTemplate::new(404)
            } else {
                ResponseTemplate::new(200).set_body_string(sample(y))
            };
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/PublicHolidays/{y}/DE")))
                .respond_with(response)
                .mount(&server)
                .await;
        }
    });
    std::env::set_var("AWAYD_NAGER_API_BASE", server.uri());
    let err = sync::refresh_country_sync("DE").expect_err("required year missing must fail");
    assert!(err.contains(&format!("{this_year}")), "{err}");
    assert_eq!(std::fs::read_to_string(&cache).unwrap(), before);
}
