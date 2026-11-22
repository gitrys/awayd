//! SPDX-License-Identifier: GPL-3.0-or-later
//! Convert Nager API responses into sdss holiday JSON (schema v3).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::Utc;

use super::types::PublicHoliday;
use crate::holidays::data::{HolidayFile, HolidayRecord, SubdivEntry, YearEntry};

const PUBLIC: &str = "public";
const SOURCE: &str = "nager.date";

/// Normalize Nager county code (e.g. `DE-BY`) to config subdiv key (`BY`).
pub fn normalize_subdiv(country: &str, county: &str) -> String {
    let country = country.to_uppercase();
    let county = county.trim().to_uppercase();
    if let Some(rest) = county.strip_prefix(&format!("{country}-")) {
        return rest.to_string();
    }
    county
}

fn is_public(h: &PublicHoliday) -> bool {
    h.types.iter().any(|t| t.eq_ignore_ascii_case("public"))
}

fn record_from_holiday(h: &PublicHoliday) -> HolidayRecord {
    let name = h.name.clone().unwrap_or_else(|| h.date.clone());
    HolidayRecord::Named {
        date: h.date.clone(),
        name,
        local_name: h.local_name.clone(),
    }
}

/// Build one year's `by_subdiv` from Nager holidays.
pub fn year_from_holidays(
    country: &str,
    holidays: &[PublicHoliday],
) -> HashMap<String, SubdivEntry> {
    let country = country.to_uppercase();
    let mut subdivs: BTreeSet<String> = BTreeSet::new();
    for h in holidays {
        if !is_public(h) {
            continue;
        }
        if h.global {
            continue;
        }
        if let Some(ref counties) = h.counties {
            for c in counties {
                subdivs.insert(normalize_subdiv(&country, c));
            }
        }
    }
    if subdivs.is_empty() {
        subdivs.insert(String::new());
    }

    let mut by_subdiv: HashMap<String, BTreeMap<String, HolidayRecord>> = HashMap::new();
    for s in &subdivs {
        by_subdiv.insert(s.clone(), BTreeMap::new());
    }

    for h in holidays {
        if !is_public(h) {
            continue;
        }
        let rec = record_from_holiday(h);
        if h.global {
            for s in by_subdiv.keys().cloned().collect::<Vec<_>>() {
                by_subdiv
                    .get_mut(&s)
                    .unwrap()
                    .insert(h.date.clone(), rec.clone());
            }
        } else if let Some(ref counties) = h.counties {
            for c in counties {
                let sk = normalize_subdiv(&country, c);
                if let Some(map) = by_subdiv.get_mut(&sk) {
                    map.insert(h.date.clone(), rec.clone());
                }
            }
        }
    }

    by_subdiv
        .into_iter()
        .map(|(k, records)| {
            let list: Vec<HolidayRecord> = records.into_values().collect();
            (
                k,
                SubdivEntry {
                    by_category: HashMap::from([(PUBLIC.to_string(), list)]),
                },
            )
        })
        .collect()
}

/// Merge multiple years into a v3 [`HolidayFile`].
pub fn build_holiday_file(country: &str, years: &[(i32, Vec<PublicHoliday>)]) -> HolidayFile {
    let country = country.to_uppercase();
    let mut by_year = HashMap::new();
    let mut year_keys: Vec<i32> = Vec::new();
    for (y, holidays) in years {
        year_keys.push(*y);
        by_year.insert(
            y.to_string(),
            YearEntry {
                by_subdiv: year_from_holidays(&country, holidays),
            },
        );
    }
    year_keys.sort_unstable();
    let year_min = *year_keys.first().unwrap_or(&0);
    let year_max = *year_keys.last().unwrap_or(&0);
    HolidayFile {
        schema_version: 3,
        country: country.clone(),
        source: Some(SOURCE.to_string()),
        fetched_at: Some(Utc::now().to_rfc3339()),
        year_min: Some(year_min),
        year_max: Some(year_max),
        by_year,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holidays::data::HolidayRecord;

    #[test]
    fn normalize_de_county() {
        assert_eq!(normalize_subdiv("DE", "DE-BY"), "BY");
        assert_eq!(normalize_subdiv("DE", "BY"), "BY");
    }

    #[test]
    fn global_applies_to_all_subdivs() {
        let holidays = vec![
            PublicHoliday {
                date: "2026-01-01".into(),
                name: Some("New Year's Day".into()),
                local_name: Some("Neujahr".into()),
                global: true,
                counties: None,
                types: vec!["Public".into()],
            },
            PublicHoliday {
                date: "2026-08-15".into(),
                name: Some("Assumption Day".into()),
                local_name: Some("Mariä Himmelfahrt".into()),
                global: false,
                counties: Some(vec!["DE-BY".into()]),
                types: vec!["Public".into()],
            },
        ];
        let m = year_from_holidays("DE", &holidays);
        assert!(m.contains_key("BY"));
        let by = &m["BY"].by_category["public"];
        assert!(by.iter().any(|r| r.date_str() == "2026-01-01"));
        assert!(by.iter().any(|r| r.date_str() == "2026-08-15"));
        assert!(matches!(
            by.iter().find(|r| r.date_str() == "2026-01-01"),
            Some(HolidayRecord::Named { local_name: Some(n), .. }) if n == "Neujahr"
        ));
    }
}
