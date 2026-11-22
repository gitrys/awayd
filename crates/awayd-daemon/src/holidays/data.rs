//! SPDX-License-Identifier: GPL-3.0-or-later
//! Layered holiday JSON loader (Nager.Date cache + optional system dir).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use awayd_core::config::paths;

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

/// Where holiday data came from (for `awayctl holidays list` / doctor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolidaySource {
    UserData,
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

    for (code, bytes) in EMBEDDED_HOLIDAY_FILES {
        if code.eq_ignore_ascii_case(country) {
            return Some((bytes.to_vec(), HolidaySource::Embedded));
        }
    }
    None
}

pub(crate) fn parse_holiday_file(raw: &[u8]) -> Result<HolidayFile, String> {
    let file: HolidayFile = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    if file.schema_version != 1 && file.schema_version != 2 && file.schema_version != 3 {
        return Err(format!(
            "unsupported holiday file schema_version {}",
            file.schema_version
        ));
    }
    Ok(file)
}

pub(crate) fn file_year_range(file: &HolidayFile) -> (i32, i32) {
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
    let mut dates = HashSet::new();
    for rec in records_for(yr, subdiv) {
        let nd =
            NaiveDate::parse_from_str(rec.date_str(), "%Y-%m-%d").map_err(|e| e.to_string())?;
        dates.insert(nd);
    }
    Ok(HolidaySet { dates })
}

fn subdiv_key(subdiv: &str) -> String {
    subdiv.trim().to_uppercase()
}

fn subdiv_entry_for<'a>(yr: &'a YearEntry, subdiv: &str) -> Option<&'a SubdivEntry> {
    yr.by_subdiv
        .get(&subdiv_key(subdiv))
        .or_else(|| yr.by_subdiv.get(subdiv))
}

/// Public holidays of `subdiv` in one year. An empty or unknown region (typo, region that has
/// no regional holiday in this year) falls back to the holidays every region of the year shares,
/// i.e. the nationwide ones; before, it yielded no holiday at all, not even Christmas.
fn records_for<'a>(yr: &'a YearEntry, subdiv: &str) -> Vec<&'a HolidayRecord> {
    match subdiv_entry_for(yr, subdiv) {
        Some(sub) => sub
            .by_category
            .get(HOLIDAY_CATEGORY)
            .map(|list| list.iter().collect())
            .unwrap_or_default(),
        None => nationwide_records(yr),
    }
}

/// Holidays present in every region of the year (works with existing cache files, which have
/// no separate "" entry for countries with regions).
fn nationwide_records(yr: &YearEntry) -> Vec<&HolidayRecord> {
    let lists: Vec<&[HolidayRecord]> = yr
        .by_subdiv
        .values()
        .map(|s| {
            s.by_category
                .get(HOLIDAY_CATEGORY)
                .map(Vec::as_slice)
                .unwrap_or(&[])
        })
        .collect();
    let Some((first, rest)) = lists.split_first() else {
        return Vec::new();
    };
    let rest: Vec<HashSet<&str>> = rest
        .iter()
        .map(|l| l.iter().map(HolidayRecord::date_str).collect())
        .collect();
    first
        .iter()
        .filter(|r| rest.iter().all(|dates| dates.contains(r.date_str())))
        .collect()
}

/// Status warning when `subdiv` is not a region of the holiday file (in any year).
fn region_warning_for(file: &HolidayFile, country: &str, subdiv: &str) -> Option<String> {
    let key = subdiv_key(subdiv);
    let known = file
        .by_year
        .values()
        .any(|yr| yr.by_subdiv.contains_key(&key) || yr.by_subdiv.contains_key(subdiv));
    if known {
        return None;
    }
    let country = country.trim().to_uppercase();
    Some(if key.is_empty() {
        format!(
            "no region set for {country} (schedule.subdiv); only nationwide public holidays apply"
        )
    } else {
        format!(
            "region {subdiv:?} not found in the holiday data for {country}; only nationwide public holidays apply"
        )
    })
}

fn region_warning(country: &str, subdiv: &str) -> Option<String> {
    let (raw, _) = resolve_country_bytes(country)?;
    let file = parse_holiday_file(&raw).ok()?;
    region_warning_for(&file, country, subdiv)
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
    let mut out: Vec<HolidayListEntry> = records_for(yr, subdiv)
        .into_iter()
        .map(HolidayRecord::to_list_entry)
        .collect();
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

/// Warnings when the holiday data may not cover the scheduling horizon (`year` + `year+1`).
/// A missing year otherwise yields an empty holiday set without any notice.
pub fn holiday_coverage_warnings(country: &str, year: i32) -> Vec<String> {
    let country = country.trim();
    if country.is_empty() {
        return vec!["schedule.country is empty; no holiday layer checked".to_string()];
    }
    let y = year;
    let y_next = y + 1;
    let Some((_, src)) = resolve_country_bytes(country) else {
        return vec![format!(
            "no holiday data for country {country:?} (connect to the internet; awayd downloads it from Nager.Date into {})",
            paths::holidays_dir().display()
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
            "holiday data for {country} ({src:?}) does not cover {y} (file years {min_y}..{max_y}); public holidays in {y} are ignored until the data is refreshed"
        ));
    }
    if y_next > max_y {
        out.push(format!(
            "holiday data for {country} ({src:?}) does not cover {y_next} (file max {max_y}); holidays early in {y_next} are ignored until the data is refreshed"
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

/// Like [`load_holiday_nearby_years`], but missing or unreadable holiday data is not fatal:
/// returns an empty set plus a warning for the status. Without this,
/// a first start without network applied no status at all and manual overrides failed.
pub fn load_holidays_or_empty(
    country: &str,
    subdiv: &str,
    year: i32,
) -> (HolidaySet, Option<String>) {
    match load_holiday_nearby_years(country, subdiv, year) {
        // Data present, but a missing year would silently yield no holidays: report it.
        Ok(set) => {
            let mut warnings = holiday_coverage_warnings(country, year);
            warnings.extend(region_warning(country, subdiv));
            (set, (!warnings.is_empty()).then(|| warnings.join("; ")))
        }
        Err(e) => (
            HolidaySet::default(),
            Some(format!(
                "{e}; public holidays are ignored until holiday data is available"
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(dates: &[&str]) -> SubdivEntry {
        SubdivEntry {
            by_category: HashMap::from([(
                HOLIDAY_CATEGORY.to_string(),
                dates
                    .iter()
                    .map(|d| HolidayRecord::DateOnly(d.to_string()))
                    .collect(),
            )]),
        }
    }

    /// Like the DE cache: regions only, no "" entry.
    fn de_year() -> YearEntry {
        YearEntry {
            by_subdiv: HashMap::from([
                (
                    "BW".to_string(),
                    region(&["2026-01-01", "2026-01-06", "2026-12-25"]),
                ),
                (
                    "BY".to_string(),
                    region(&["2026-01-01", "2026-08-15", "2026-12-25"]),
                ),
            ]),
        }
    }

    fn dates(records: Vec<&HolidayRecord>) -> Vec<&str> {
        let mut d: Vec<&str> = records.into_iter().map(HolidayRecord::date_str).collect();
        d.sort_unstable();
        d
    }

    #[test]
    fn known_region_keeps_its_own_holidays() {
        let yr = de_year();
        assert_eq!(
            dates(records_for(&yr, "BW")),
            ["2026-01-01", "2026-01-06", "2026-12-25"]
        );
        assert_eq!(dates(records_for(&yr, "bw")), dates(records_for(&yr, "BW")));
    }

    #[test]
    fn empty_or_unknown_region_falls_back_to_nationwide() {
        let yr = de_year();
        for subdiv in ["", "BWX"] {
            assert_eq!(
                dates(records_for(&yr, subdiv)),
                ["2026-01-01", "2026-12-25"],
                "{subdiv:?}"
            );
        }
    }

    #[test]
    fn country_without_regions_uses_its_single_entry() {
        let yr = YearEntry {
            by_subdiv: HashMap::from([(String::new(), region(&["2026-10-26"]))]),
        };
        assert_eq!(dates(records_for(&yr, "")), ["2026-10-26"]);
        assert_eq!(dates(records_for(&yr, "W")), ["2026-10-26"]);
    }

    #[test]
    fn region_warning_only_for_regions_missing_in_all_years() {
        let file = HolidayFile {
            schema_version: 3,
            country: "DE".into(),
            source: None,
            fetched_at: None,
            year_min: Some(2026),
            year_max: Some(2026),
            by_year: HashMap::from([("2026".to_string(), de_year())]),
        };
        assert_eq!(region_warning_for(&file, "DE", "BW"), None);
        assert_eq!(region_warning_for(&file, "de", "by"), None);
        let typo = region_warning_for(&file, "DE", "BWX").unwrap();
        assert!(
            typo.contains("\"BWX\"") && typo.contains("nationwide"),
            "{typo}"
        );
        let empty = region_warning_for(&file, "DE", "").unwrap();
        assert!(empty.contains("no region set for DE"), "{empty}");
    }
}
