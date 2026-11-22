//! SPDX-License-Identifier: GPL-3.0-or-later
//! The async holiday sync entry points (background task, IPC) must work on a multi-threaded
//! Tokio runtime. Calling the blocking `*_sync` functions there directly panicked in debug
//! builds ("Cannot drop a runtime in a context where blocking is not allowed") and blocked a
//! worker in release builds.
//!
//! One test function on purpose: it sets process-wide env vars (APPDATA, Nager base URL),
//! and every file in `tests/` is its own process.

use awayd_daemon::holidays::sync;
use chrono::Utc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SAMPLE: &str = r#"[
  {
    "date": "2026-12-25",
    "localName": "1. Weihnachtstag",
    "name": "Christmas Day",
    "countryCode": "DE",
    "global": true,
    "counties": null,
    "types": ["Public"]
  }
]"#;

const COUNTRIES: &str = r#"[{"countryCode":"DE","name":"Germany"}]"#;

#[test]
fn async_sync_entry_points_run_on_a_multi_thread_runtime() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("APPDATA", dir.path());
    std::env::remove_var("AWAYD_PORTABLE");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let server = MockServer::start().await;
        let (y0, y1) = sync::year_window(Utc::now());
        for y in y0..=y1 {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/PublicHolidays/{y}/DE")))
                .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/AvailableCountries"))
            .respond_with(ResponseTemplate::new(200).set_body_string(COUNTRIES))
            .mount(&server)
            .await;
        std::env::set_var("AWAYD_NAGER_API_BASE", server.uri());

        // Same call as the background task: runs on a Tokio worker.
        sync::ensure_country("DE")
            .await
            .expect("ensure_country on a multi-thread runtime");
        assert!(sync::cache_path("DE").is_file());

        // IPC paths.
        sync::refresh_country("DE")
            .await
            .expect("refresh_country on a multi-thread runtime");
        let countries = sync::list_available_countries_cached()
            .await
            .expect("countries on a multi-thread runtime");
        assert_eq!(countries, vec![("DE".to_string(), "Germany".to_string())]);
    });
}
