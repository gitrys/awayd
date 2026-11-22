//! SPDX-License-Identifier: GPL-3.0-or-later
//! Fetch and cache holiday JSON from Nager.Date.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{Datelike, Utc};
use std::sync::Mutex;

use sdss_core::config::paths;

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
/// Uses blocking HTTP so it is safe from the zbus D-Bus executor (no Tokio reactor).
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
        target: "sdss::holidays",
        country,
        y0,
        y1,
        path = %path.display(),
        "cached holiday data from Nager.Date"
    );
    Ok(())
}

pub async fn ensure_country(country: &str) -> Result<(), String> {
    ensure_country_sync(country)
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

pub async fn refresh_country(country: &str) -> Result<(), String> {
    refresh_country_sync(country)
}

fn write_holiday_file(path: &Path, file: &HolidayFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    sdss_core::atomic_fs::atomic_write(path, json.as_bytes()).map_err(|e| e.to_string())
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

pub async fn list_available_countries_cached() -> Result<Vec<(String, String)>, String> {
    list_available_countries_cached_sync()
}

/// Scan `~/.local/share/sdss/holidays/*.json`.
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

/// Background loop: sync config country, recheck weekly.
pub async fn background_task() {
    loop {
        let country = sdss_core::config::load_config(&paths::config_file())
            .map(|c| c.schedule.country)
            .unwrap_or_else(|_| "DE".to_string());
        if let Err(e) = ensure_country_sync(&country) {
            tracing::warn!(
                target: "sdss::holidays",
                country = %country,
                error = %e,
                "holiday sync failed"
            );
        }
        tokio::time::sleep(Duration::from_secs(7 * 24 * 3600)).await;
    }
}
