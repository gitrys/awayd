//! SPDX-License-Identifier: GPL-3.0-or-later
//! Schedule parsing helpers shared by daemon and settings UI.

use chrono::NaiveTime;
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

pub fn parse_hhmm(s: &str) -> Result<NaiveTime, String> {
    let s = s.trim();
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(\d{1,2}):(\d{2})$").unwrap());
    let c = re
        .captures(s)
        .ok_or_else(|| format!("Invalid time (expected HH:MM): {s:?}"))?;
    let h: u32 = c[1].parse().map_err(|_| "hour".to_string())?;
    let mi: u32 = c[2].parse().map_err(|_| "minute".to_string())?;
    if h > 23 || mi > 59 {
        return Err(format!("Invalid time values: {s:?}"));
    }
    NaiveTime::from_hms_opt(h, mi, 0).ok_or_else(|| format!("Invalid time: {s:?}"))
}

pub fn parse_weekdays(spec: &str) -> Result<HashSet<u8>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("weekdays is empty".into());
    }
    let mut out: HashSet<u8> = HashSet::new();
    for raw in spec.split(',') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((a, b)) = part.split_once('-') {
            let mut lo: u8 = a
                .trim()
                .parse()
                .map_err(|_| format!("bad weekday token {part:?}"))?;
            let mut hi: u8 = b
                .trim()
                .parse()
                .map_err(|_| format!("bad weekday token {part:?}"))?;
            if lo > hi {
                std::mem::swap(&mut lo, &mut hi);
            }
            for d in lo..=hi {
                out.insert(d);
            }
        } else {
            let d: u8 = part
                .parse()
                .map_err(|_| format!("bad weekday token {part:?}"))?;
            out.insert(d);
        }
    }
    for &d in &out {
        if !(1..=7).contains(&d) {
            return Err(format!("Weekday out of range 1-7: {d}"));
        }
    }
    if out.is_empty() {
        return Err("weekdays resolved to empty set".into());
    }
    Ok(out)
}
