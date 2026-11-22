//! SPDX-License-Identifier: GPL-3.0-or-later
//! Holiday catalogue (subdivisions from cached JSON).

use std::collections::HashSet;

use super::data::{parse_holiday_file, resolve_country_bytes, HolidayFile};

pub fn subdivisions_for_country(country: &str) -> Vec<String> {
    let Some((raw, _)) = resolve_country_bytes(country) else {
        return vec![];
    };
    let Ok(file) = parse_holiday_file(&raw) else {
        return vec![];
    };
    subdivisions_from_file(&file)
}

pub(crate) fn subdivisions_from_file(file: &HolidayFile) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    for y in file.by_year.values() {
        for k in y.by_subdiv.keys() {
            if k.is_empty() {
                continue;
            }
            set.insert(k.clone());
        }
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}
