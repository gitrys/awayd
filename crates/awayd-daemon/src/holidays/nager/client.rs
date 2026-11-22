//! SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP client for Nager.Date API v3.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::blocking::Client as BlockingClient;

use super::types::{AvailableCountry, PublicHoliday};

fn base_url() -> String {
    std::env::var("AWAYD_NAGER_API_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://date.nager.at".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Blocking client: async callers go through `holidays::sync` (`spawn_blocking`).
fn blocking_http_client() -> &'static BlockingClient {
    static CLIENT: OnceLock<BlockingClient> = OnceLock::new();
    CLIENT.get_or_init(|| {
        BlockingClient::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest blocking client")
    })
}

/// Blocking fetch for callers without a Tokio runtime.
pub fn fetch_available_countries_blocking() -> Result<Vec<AvailableCountry>, String> {
    let url = format!("{}/api/v3/AvailableCountries", base_url());
    let resp = blocking_http_client()
        .get(&url)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("AvailableCountries HTTP {}", resp.status()));
    }
    resp.json::<Vec<AvailableCountry>>()
        .map_err(|e| e.to_string())
}

pub fn fetch_public_holidays_blocking(
    year: i32,
    country: &str,
) -> Result<Vec<PublicHoliday>, String> {
    let cc = country.to_uppercase();
    let url = format!("{}/api/v3/PublicHolidays/{year}/{cc}", base_url());
    let resp = blocking_http_client()
        .get(&url)
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("PublicHolidays/{year}/{cc} HTTP {}", resp.status()));
    }
    resp.json::<Vec<PublicHoliday>>().map_err(|e| e.to_string())
}
