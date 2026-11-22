//! SPDX-License-Identifier: GPL-3.0-or-later
//! Layered holiday JSON loader (Nager.Date cache + optional system dir).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use sdss_core::config::paths;

include!(concat!(env!("OUT_DIR"), "/embedded_holidays.rs"));

const HOLIDAY_CATEGORY: &str = "public";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HolidayFile {
    pub(crate) schema_version: u32,
    #[allow(dead_code)]
    pub(crate) country: String,
    #[serde(default)]
    pub(crate) source: Option<String>,
    #[serde(default)]
    pub(crate) fetched_at: Option<String>,
    #[serde(default)]
    pub(crate) year_min: Option<i32>,
    #[serde(default)]
    pub(crate) year_max: Option<i32>,
    pub(crate) by_year: HashMap<String, YearEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct YearEntry {
    pub(crate) by_subdiv: HashMap<String, SubdivEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubdivEntry {
    pub(crate) by_category: HashMap<String, Vec<HolidayRecord>>,
}

/// One public holiday row in cache JSON (v3 named object or legacy date string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(crate) enum HolidayRecord {
    DateOnly(String),
    Named {
        date: String,
        name: String,
        #[serde(default)]
        local_name: Option<String>,
    },
}

impl HolidayRecord {
    pub(crate) fn date_str(&self) -> &str {
        match self {
            Self::DateOnly(d) => d,
            Self::Named { date, .. } => date,
        }
    }

    pub fn to_list_entry(&self) -> HolidayListEntry {
        match self {
            Self::DateOnly(d) => HolidayListEntry {
                date: d.clone(),
                name: d.clone(),
                local_name: None,
            },
            Self::Named {
                date,
                name,
                local_name,
            } => HolidayListEntry {
                date: date.clone(),
                name: name.clone(),
                local_name: local_name.clone(),
            },
        }
    }
}

/// Named holiday row for UI / IPC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HolidayListEntry {
    pub date: String,
    pub name: String,
    pub local_name: Option<String>,
}

impl HolidayListEntry {
    /// Primary label: local name only; legacy rows fall back to the date.
    pub fn display_name(&self) -> &str {
        self.local_name.as_deref().unwrap_or(self.date.as_str())
    }

    pub fn has_proper_name(&self) -> bool {
        self.local_name.is_some()
    }
}

/// Where holiday data came from (for `sdss holidays list` / doctor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolidaySource {
    UserData,
    System,
    Embedded,
}

pub fn cache_file_path(country: &str) -> std::path::PathBuf {
    let c = country.to_uppercase();
    paths::holidays_dir().join(format!("{c}.json"))
}

pub fn resolve_country_bytes(country: &str) -> Option<(Vec<u8>, HolidaySource)> {
    let p = cache_file_path(country);
    if let Ok(b) = std::fs::read(&p) {
        return Some((b, HolidaySource::UserData));
    }

    let fname = format!("{}.json", country.to_uppercase());
    let sys = Path::new("/usr/share/sdss/holidays").join(&fname);
    if let Ok(b) = std::fs::read(&sys) {
        return Some((b, HolidaySource::System));
    }

    for (code, bytes) in EMBEDDED_HOLIDAY_FILES {
        if code.eq_ignore_ascii_case(country) {
            return Some((bytes.to_vec(), HolidaySource::Embedded));
        }
    }
    None
}

pub fn parse_holiday_file(raw: &[u8]) -> Result<HolidayFile, String> {
    let file: HolidayFile = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    if file.schema_version != 1 && file.schema_version != 2 && file.schema_version != 3 {
        return Err(format!(
            "unsupported holiday file schema_version {}",
            file.schema_version
        ));
    }
    Ok(file)
}

pub fn file_year_range(file: &HolidayFile) -> (i32, i32) {
    if let (Some(a), Some(b)) = (file.year_min, file.year_max) {
        return (a, b);
    }
    let mut years: Vec<i32> = file.by_year.keys().filter_map(|s| s.parse().ok()).collect();
    years.sort_unstable();
    match (years.first(), years.last()) {
        (Some(a), Some(b)) => (*a, *b),
        _ => (0, 0),
    }
}

static HOLIDAY_SET_CACHE: OnceLock<Mutex<HashMap<String, HolidaySet>>> = OnceLock::new();

pub fn clear_holiday_cache() {
    if let Some(cache) = HOLIDAY_SET_CACHE.get() {
        if let Ok(mut g) = cache.lock() {
            g.clear();
        }
    }
}

fn holiday_cache_key(country: &str, subdiv: &str, year: i32) -> String {
    format!("{}|{}|{}", country.to_uppercase(), subdiv, year)
}

pub fn load_holiday_set(country: &str, subdiv: &str, year: i32) -> Result<HolidaySet, String> {
    let key = holiday_cache_key(country, subdiv, year);
    let cache = HOLIDAY_SET_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(g) = cache.lock() {
        if let Some(v) = g.get(&key) {
            return Ok(v.clone());
        }
    }
    let out = load_holiday_set_uncached(country, subdiv, year);
    if let Ok(ref set) = out {
        if let Ok(mut g) = cache.lock() {
            if g.len() > crate::holidays::lru::HOLIDAY_CACHE_CAP {
                g.clear();
            }
            g.insert(key, set.clone());
        }
    }
    out
}

fn load_holiday_set_uncached(country: &str, subdiv: &str, year: i32) -> Result<HolidaySet, String> {
    let Some((raw, _src)) = resolve_country_bytes(country) else {
        return Err(format!("no holiday data for country {country}"));
    };
    let file = parse_holiday_file(&raw)?;
    let year_s = year.to_string();
    let Some(yr) = file.by_year.get(&year_s) else {
        return Ok(HolidaySet {
            dates: HashSet::new(),
        });
    };
    let Some(sub) = subdiv_entry_for(yr, subdiv) else {
        return Ok(HolidaySet {
            dates: HashSet::new(),
        });
    };
    let mut dates = HashSet::new();
    if let Some(list) = sub.by_category.get(HOLIDAY_CATEGORY) {
        for rec in list {
            let nd =
                NaiveDate::parse_from_str(rec.date_str(), "%Y-%m-%d").map_err(|e| e.to_string())?;
            dates.insert(nd);
        }
    }
    Ok(HolidaySet { dates })
}

fn subdiv_entry_for<'a>(yr: &'a YearEntry, subdiv: &str) -> Option<&'a SubdivEntry> {
    let subdiv_key = if subdiv.is_empty() {
        "".to_string()
    } else {
        subdiv.to_uppercase()
    };
    yr.by_subdiv
        .get(&subdiv_key)
        .or_else(|| yr.by_subdiv.get(subdiv))
}

/// List public holidays for `(country, subdiv, year)`, sorted by date.
pub fn list_holidays_for_subdiv(
    country: &str,
    subdiv: &str,
    year: i32,
) -> Result<Vec<HolidayListEntry>, String> {
    let Some((raw, _src)) = resolve_country_bytes(country) else {
        return Err(format!("no holiday data for country {country}"));
    };
    let file = parse_holiday_file(&raw)?;
    let year_s = year.to_string();
    let Some(yr) = file.by_year.get(&year_s) else {
        return Ok(Vec::new());
    };
    let Some(sub) = subdiv_entry_for(yr, subdiv) else {
        return Ok(Vec::new());
    };
    let mut out: Vec<HolidayListEntry> = sub
        .by_category
        .get(HOLIDAY_CATEGORY)
        .map(|list| list.iter().map(HolidayRecord::to_list_entry).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(out)
}

/// In-memory set for one (country, subdiv, year).
#[derive(Debug, Clone, Default)]
pub struct HolidaySet {
    pub(crate) dates: HashSet<NaiveDate>,
}

/// Public holiday counts for one region and calendar year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HolidayYearSummary {
    pub total: u32,
    pub remaining: u32,
}

/// Count holidays in `year` for `(country, subdiv)`; `remaining` is on or after `today`.
pub fn holiday_year_summary_on(
    country: &str,
    subdiv: &str,
    year: i32,
    today: NaiveDate,
) -> Result<HolidayYearSummary, String> {
    let set = load_holiday_set(country, subdiv, year)?;
    let total = set.dates.len() as u32;
    let remaining = set
        .dates
        .iter()
        .filter(|d| d.year() == year && **d >= today)
        .count() as u32;
    Ok(HolidayYearSummary { total, remaining })
}

/// Same as [`holiday_year_summary_on`] using the local calendar date as `today`.
pub fn holiday_year_summary(
    country: &str,
    subdiv: &str,
    year: i32,
) -> Result<HolidayYearSummary, String> {
    holiday_year_summary_on(country, subdiv, year, chrono::Local::now().date_naive())
}

pub fn year_range_in_file(country: &str) -> Option<(i32, i32)> {
    let (raw, _) = resolve_country_bytes(country)?;
    let file = parse_holiday_file(&raw).ok()?;
    let (a, b) = file_year_range(&file);
    if a == 0 && b == 0 {
        return None;
    }
    Some((a, b))
}

/// Warnings when the active holiday layer may not cover the scheduling horizon (`year` + `year+1`).
pub fn holiday_coverage_warnings(country: &str) -> Vec<String> {
    let country = country.trim();
    if country.is_empty() {
        return vec!["schedule.country is empty; no holiday layer checked".to_string()];
    }
    let y = chrono::Utc::now().date_naive().year();
    let y_next = y + 1;
    let Some((_, src)) = resolve_country_bytes(country) else {
        return vec![format!(
            "no holiday data for country {country:?} (connect to the internet; sdssd caches from Nager.Date under ~/.local/share/sdss/holidays/)"
        )];
    };
    let Some((min_y, max_y)) = year_range_in_file(country) else {
        return vec![format!(
            "holiday file for {country} ({src:?}) could not be parsed for year range"
        )];
    };
    let mut out = Vec::new();
    if y < min_y || y > max_y {
        out.push(format!(
            "holiday data for {country} ({src:?}) may not cover current calendar year {y} (file years {min_y}..{max_y})"
        ));
    }
    if y_next > max_y {
        out.push(format!(
            "holiday data for {country} ({src:?}) may not cover next year {y_next}; schedule merges year+year+1 (file max {max_y})"
        ));
    }
    out
}

impl HolidaySet {
    pub fn is_holiday(&self, d: NaiveDate) -> bool {
        self.dates.contains(&d)
    }

    fn merge(&mut self, other: HolidaySet) {
        self.dates.extend(other.dates);
    }
}

/// Load holidays for `year` and `year + 1` (schedule horizon crosses New Year).
pub fn load_holiday_nearby_years(
    country: &str,
    subdiv: &str,
    year: i32,
) -> Result<HolidaySet, String> {
    let mut a = load_holiday_set(country, subdiv, year)?;
    if let Ok(b) = load_holiday_set(country, subdiv, year + 1) {
        a.merge(b);
    }
    Ok(a)
}
