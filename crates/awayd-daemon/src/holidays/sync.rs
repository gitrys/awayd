//! SPDX-License-Identifier: GPL-3.0-or-later
//! Fetch and cache holiday JSON from Nager.Date.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{Datelike, Utc};
use std::sync::Mutex;

use awayd_core::config::paths;

use super::data::{self, clear_holiday_cache, HolidayFile};
use super::nager::{build_holiday_file, fetch_public_holidays_blocking};

const REFRESH_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

static COUNTRIES_CACHE: OnceLock<Mutex<Option<(Instant, Vec<(String, String)>)>>> = OnceLock::new();

pub fn cache_path(country: &str) -> PathBuf {
    let cc = country.trim().to_uppercase();
    paths::holidays_dir().join(format!("{cc}.json"))
}

/// Inclusive year range: `current - 1` .. `current + 3`.
pub fn year_window(now: chrono::DateTime<Utc>) -> (i32, i32) {
    let y = now.date_naive().year();
    (y - 1, y + 3)
}

pub fn needs_refresh(path: &Path) -> bool {
    if !path.is_file() {
        return true;
    }
    let Ok(raw) = std::fs::read(path) else {
        return true;
    };
    let Ok(file) = serde_json::from_slice::<HolidayFile>(&raw) else {
        return true;
    };
    let now = Utc::now();
    let y = now.date_naive().year();
    let y_next = y + 1;
    let (min_y, max_y) = data::file_year_range(&file);
    if y < min_y || y_next > max_y {
        return true;
    }
    if let Some(ref at) = file.fetched_at {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(at) {
            let age = now.signed_duration_since(parsed.with_timezone(&Utc));
            if age.to_std().unwrap_or(Duration::ZERO) > REFRESH_MAX_AGE {
                return true;
            }
        } else {
            return true;
        }
    } else {
        return true;
    }
    false
}

/// Fetch years from Nager and write `paths::holidays_dir()/{CC}.json`.
/// Blocking HTTP: call it from a plain thread, or use [`ensure_country`] in async code.
pub fn ensure_country_sync(country: &str) -> Result<(), String> {
    sync_country(country, false)
}

/// Download unless the cache is fresh (or always with `force`). The existing file is only
/// replaced (atomically) after every year was fetched; on error it stays untouched.
fn sync_country(country: &str, force: bool) -> Result<(), String> {
    let country = country.trim();
    if country.is_empty() {
        return Err("country code is empty".into());
    }
    let path = cache_path(country);
    if !force && !needs_refresh(&path) {
        return Ok(());
    }
    std::fs::create_dir_all(paths::holidays_dir()).map_err(|e| e.to_string())?;

    let now = Utc::now();
    let (y0, y1) = year_window(now);
    let mut years: Vec<(i32, Vec<super::nager::types::PublicHoliday>)> = Vec::new();
    for y in y0..=y1 {
        let holidays = fetch_public_holidays_blocking(y, country)?;
        years.push((y, holidays));
    }
    let file = build_holiday_file(country, &years);
    write_holiday_file(&path, &file)?;
    clear_holiday_cache();
    tracing::info!(
        target: "awayd::holidays",
        country,
        y0,
        y1,
        path = %path.display(),
        "cached holiday data from Nager.Date"
    );
    Ok(())
}

/// Run blocking holiday work (HTTP via `reqwest::blocking`, file I/O) on Tokio's blocking pool.
/// Calling the `*_sync` functions directly on a Tokio worker blocks it for up to minutes and
/// panics in debug builds (reqwest drops its internal runtime inside an async context).
async fn off_runtime<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("holiday task failed: {e}"))?
}

/// [`ensure_country_sync`] for async callers (background task).
pub async fn ensure_country(country: &str) -> Result<(), String> {
    let country = country.to_string();
    off_runtime(move || ensure_country_sync(&country)).await
}

/// Force refresh even if cache looks fresh. On failure the previous file is kept
/// (it used to be deleted before the download).
pub fn refresh_country_sync(country: &str) -> Result<(), String> {
    sync_country(country, true).map_err(|e| {
        if cache_path(country).is_file() {
            format!("{e} (existing holiday data stays active)")
        } else {
            e
        }
    })
}

/// [`refresh_country_sync`] for async callers (IPC).
pub async fn refresh_country(country: &str) -> Result<(), String> {
    let country = country.to_string();
    off_runtime(move || refresh_country_sync(&country)).await
}

fn write_holiday_file(path: &Path, file: &HolidayFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    awayd_core::atomic_fs::atomic_write(path, json.as_bytes()).map_err(|e| e.to_string())
}

pub fn list_available_countries_cached_sync() -> Result<Vec<(String, String)>, String> {
    let cache = COUNTRIES_CACHE.get_or_init(|| Mutex::new(None));
    let mut g = cache.lock().map_err(|e| e.to_string())?;
    if let Some((t, ref list)) = *g {
        if t.elapsed() < Duration::from_secs(24 * 3600) {
            return Ok(list.clone());
        }
    }
    let list = super::nager::fetch_available_countries_blocking()?
        .into_iter()
        .map(|c| (c.country_code, c.name))
        .collect::<Vec<_>>();
    *g = Some((Instant::now(), list.clone()));
    Ok(list)
}

/// [`list_available_countries_cached_sync`] for async callers (IPC).
pub async fn list_available_countries_cached() -> Result<Vec<(String, String)>, String> {
    off_runtime(list_available_countries_cached_sync).await
}

/// Scan `{data folder}/holidays/*.json`.
pub fn list_cached_bundles() -> Vec<(String, i32, i32, String)> {
    let dir = paths::holidays_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(raw) = std::fs::read(&p) else {
            continue;
        };
        let Ok(file) = serde_json::from_slice::<HolidayFile>(&raw) else {
            continue;
        };
        let (y0, y1) = data::file_year_range(&file);
        let src = file.source.clone().unwrap_or_else(|| "cache".to_string());
        out.push((stem.to_uppercase(), y0, y1, src));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[derive(Debug, Clone)]
pub struct HolidayDataStatus {
    pub ready: bool,
    pub year_min: i32,
    pub year_max: i32,
    pub fetched_at: String,
    pub message: String,
}

pub fn holiday_data_status(country: &str) -> HolidayDataStatus {
    let country = country.trim().to_uppercase();
    if country.is_empty() {
        return HolidayDataStatus {
            ready: false,
            year_min: 0,
            year_max: 0,
            fetched_at: String::new(),
            message: "No country selected.".into(),
        };
    }
    let path = cache_path(&country);
    if !path.is_file() {
        return HolidayDataStatus {
            ready: false,
            year_min: 0,
            year_max: 0,
            fetched_at: String::new(),
            message: format!(
                "No holiday data for {country}. Connect to the internet; the daemon will download from Nager.Date automatically."
            ),
        };
    }
    let Ok(raw) = std::fs::read(&path) else {
        return HolidayDataStatus {
            ready: false,
            year_min: 0,
            year_max: 0,
            fetched_at: String::new(),
            message: format!("Could not read holiday cache for {country}."),
        };
    };
    let Ok(file) = serde_json::from_slice::<HolidayFile>(&raw) else {
        return HolidayDataStatus {
            ready: false,
            year_min: 0,
            year_max: 0,
            fetched_at: String::new(),
            message: format!("Holiday cache for {country} is invalid."),
        };
    };
    let (y0, y1) = data::file_year_range(&file);
    let fetched = file.fetched_at.clone().unwrap_or_default();
    let now = Utc::now();
    let y = now.date_naive().year();
    let y_next = y + 1;
    let mut ready = true;
    let mut message = format!("Holidays {y0}–{y1} (Nager.Date");
    if !fetched.is_empty() {
        message.push_str(&format!(", updated {fetched}"));
    }
    message.push(')');
    if y < y0 || y > y1 {
        ready = false;
        message = format!(
            "Holiday data for {country} does not cover {y} (cached {y0}–{y1}). Waiting for refresh."
        );
    } else if y_next > y1 {
        ready = false;
        message = format!(
            "Holiday data for {country} may not cover next year {y_next} (cached through {y1})."
        );
    }
    HolidayDataStatus {
        ready,
        year_min: y0,
        year_max: y1,
        fetched_at: fetched,
        message,
    }
}

const SYNC_INTERVAL_OK: Duration = Duration::from_secs(7 * 24 * 3600);
const SYNC_RETRY_FIRST: Duration = Duration::from_secs(5 * 60);
const SYNC_RETRY_MAX: Duration = Duration::from_secs(6 * 3600);

/// Wait before the next sync: weekly after success, otherwise 5 min doubling up to 6 h
/// (a failed sync used to wait a full week).
fn sync_delay(consecutive_failures: u32) -> Duration {
    if consecutive_failures == 0 {
        return SYNC_INTERVAL_OK;
    }
    let factor = 1u32 << (consecutive_failures - 1).min(16);
    SYNC_RETRY_FIRST.saturating_mul(factor).min(SYNC_RETRY_MAX)
}

/// Background loop: sync the configured country; after a download, poke the scheduler so the
/// status is recomputed with holidays (it ran without them while the data was missing).
pub async fn background_task(poke: std::sync::Arc<crate::watchers::PokeGate>) {
    let mut failures: u32 = 0;
    loop {
        let country = awayd_core::config::load_config(&paths::config_file())
            .map(|c| c.schedule.country)
            .unwrap_or_else(|_| "DE".to_string());
        let had_fresh_data = !needs_refresh(&cache_path(&country));
        match ensure_country(&country).await {
            Ok(()) => {
                failures = 0;
                if !had_fresh_data {
                    poke.poke_debounced().await;
                }
            }
            Err(e) => {
                failures = failures.saturating_add(1);
                tracing::warn!(
                    target: "awayd::holidays",
                    country = %country,
                    error = %e,
                    retry_secs = sync_delay(failures).as_secs(),
                    "holiday sync failed"
                );
            }
        }
        tokio::time::sleep(sync_delay(failures)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_delay_backs_off_after_failures() {
        assert_eq!(sync_delay(0), SYNC_INTERVAL_OK);
        assert_eq!(sync_delay(1), Duration::from_secs(5 * 60));
        assert_eq!(sync_delay(2), Duration::from_secs(10 * 60));
        assert_eq!(sync_delay(3), Duration::from_secs(20 * 60));
        assert_eq!(sync_delay(7), Duration::from_secs(320 * 60), "5 min * 64");
        assert_eq!(sync_delay(8), SYNC_RETRY_MAX, "5 min * 128 capped at 6 h");
        assert_eq!(sync_delay(u32::MAX), SYNC_RETRY_MAX, "no overflow");
    }
}
