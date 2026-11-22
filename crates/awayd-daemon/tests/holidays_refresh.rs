//! SPDX-License-Identifier: GPL-3.0-or-later
//! A forced refresh must not delete the cached holiday file
//! before the download succeeded.
//!
//! One test function on purpose: it sets process-wide env vars (APPDATA, Nager base URL),
//! and every file in `tests/` is its own process.

use awayd_daemon::holidays::sync;
use chrono::{Datelike, Utc};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OLD_CONTENT: &str = r#"{"marker":"previous holiday data"}"#;

const SAMPLE: &str = r#"[
  {
    "date": "2026-01-01",
    "localName": "Neujahr",
    "name": "New Year's Day",
    "countryCode": "DE",
    "global": true,
    "counties": null,
    "types": ["Public"]
  }
]"#;

#[test]
fn refresh_keeps_old_file_on_failure_and_replaces_it_on_success() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPDATA", dir.path());
    std::env::remove_var("AWAYD_PORTABLE");

    let cache = sync::cache_path("DE");
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(&cache, OLD_CONTENT).unwrap();

    // 1) Nager unreachable (connection refused): error, file untouched.
    std::env::set_var("AWAYD_NAGER_API_BASE", "http://127.0.0.1:9");
    let err = sync::refresh_country_sync("DE").expect_err("download must fail");
    assert!(
        err.contains("existing holiday data stays active"),
        "message: {err}"
    );
    assert_eq!(std::fs::read_to_string(&cache).unwrap(), OLD_CONTENT);

    // 2) Nager reachable: file replaced with fresh data.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());
    let (y0, y1) = sync::year_window(Utc::now());
    assert_eq!(y0, Utc::now().year() - 1);
    rt.block_on(async {
        for y in y0..=y1 {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/PublicHolidays/{y}/DE")))
                .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE))
                .mount(&server)
                .await;
        }
    });
    std::env::set_var("AWAYD_NAGER_API_BASE", server.uri());
    sync::refresh_country_sync("DE").expect("refresh with reachable Nager");

    let raw = std::fs::read_to_string(&cache).unwrap();
    assert_ne!(raw, OLD_CONTENT);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["source"].as_str(), Some("nager.date"));
}
