//! SPDX-License-Identifier: GPL-3.0-or-later
//! Next schedule flip (candidate-boundary algorithm).

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};

use crate::holidays::HolidayLookup;
use crate::local_time::{resolve_local, trim_to_minute};
use crate::work_window::{self, Config, ScheduleConfig};

const DEFAULT_MAX_DAYS: i64 = 14;

fn date_blocked(
    d: NaiveDate,
    sched: &ScheduleConfig,
    de: &dyn HolidayLookup,
) -> bool {
    work_window::date_in_vacation_ranges(d, &sched.vacation_ranges) || de.is_holiday(d)
}

/// Wall-clock boundaries are resolved DST-safe (see [`resolve_local`]); `tz` is a parameter
/// so gap/fold days can be tested with fixed time zones.
fn flip_candidates<Tz: TimeZone>(
    tz: &Tz,
    cfg: &ScheduleConfig,
    start_date: NaiveDate,
    max_days: i64,
    de: &dyn HolidayLookup,
) -> Vec<DateTime<Tz>> {
    let mut candidates = Vec::new();
    for day_offset in 0..=max_days {
        let d = start_date + Duration::days(day_offset);
        let iso = d.weekday().number_from_monday() as usize;
        let day = &cfg.days[iso - 1];

        if day.enabled && !date_blocked(d, cfg, de) {
            candidates.push(resolve_local(tz, d.and_time(day.start)));
            let end_plus = d.and_time(day.end) + Duration::minutes(1);
            candidates.push(resolve_local(tz, end_plus));
        }

        let midnight = (d + Duration::days(1)).and_time(NaiveTime::MIN);
        candidates.push(resolve_local(tz, midnight));
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Smallest `t > now` where `want_work_mode` changes vs `now`.
pub fn next_schedule_flip_after(
    cfg: &Config,
    now: DateTime<Local>,
    de: &dyn HolidayLookup,
    max_days: i64,
) -> Result<DateTime<Local>, String> {
    let now = trim_to_minute(now);
    let baseline = work_window::want_work_mode(cfg, now, de);
    let horizon = now + Duration::days(max_days);
    let sched = &cfg.schedule;

    for t in flip_candidates(&Local, sched, now.date_naive(), max_days, de) {
        let t = trim_to_minute(t);
        if t <= now {
            continue;
        }
        if t > horizon {
            break;
        }
        if work_window::want_work_mode(cfg, t, de) != baseline {
            return Ok(t);
        }
    }
    Err(format!(
        "No schedule mode change within {max_days} days after {}",
        now.format("%Y-%m-%d %H:%M:%S")
    ))
}

pub fn next_schedule_flip_after_default(
    cfg: &Config,
    now: DateTime<Local>,
    de: &dyn HolidayLookup,
) -> Result<DateTime<Local>, String> {
    next_schedule_flip_after(cfg, now, de, DEFAULT_MAX_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone, Timelike};
    use sdss_core::config::schema::DayWindow;
    use sdss_core::schedule::default_week_days;

    struct NoHolidays;

    impl HolidayLookup for NoHolidays {
        fn is_holiday(&self, _d: chrono::NaiveDate) -> bool {
            false
        }
    }

    fn cfg_with_days(mut days: [DayWindow; 7]) -> Config {
        Config {
            discord_auth_token: String::new(),
            schedule: ScheduleConfig {
                country: "DE".into(),
                subdiv: "BW".into(),
                days,
                vacation_ranges: vec![],
            },
        }
    }

    #[test]
    fn next_flip_respects_fri_short_day() {
        let mut days = default_week_days();
        days[4].end = chrono::NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let cfg = cfg_with_days(days);
        let holi = NoHolidays;

        let fri_1300 = NaiveDate::from_ymd_opt(2026, 6, 5)
            .unwrap()
            .and_hms_opt(13, 0, 0)
            .unwrap();
        let now = Local.from_local_datetime(&fri_1300).unwrap();
        let next = next_schedule_flip_after_default(&cfg, now, &holi).unwrap();
        assert_eq!(next.hour(), 14);
        assert_eq!(next.minute(), 1);
    }

    /// Midnight does not exist on DST-start day in Santiago
    /// (2027-09-05). The old `from_local_datetime(..).unwrap()` panicked for every tick
    /// whose 14-day look-ahead contained that day.
    #[test]
    fn flip_candidates_survive_midnight_dst_gap() {
        use chrono_tz::America::Santiago;
        let cfg = cfg_with_days(default_week_days());
        let start = NaiveDate::from_ymd_opt(2027, 8, 30).unwrap();
        let gap_midnight = NaiveDate::from_ymd_opt(2027, 9, 5).unwrap().and_time(NaiveTime::MIN);
        assert!(
            Santiago.from_local_datetime(&gap_midnight).earliest().is_none(),
            "precondition: midnight is in a gap"
        );

        let c = flip_candidates(&Santiago, &cfg.schedule, start, 14, &NoHolidays);

        let resolved = NaiveDate::from_ymd_opt(2027, 9, 5).unwrap().and_hms_opt(1, 0, 0).unwrap();
        assert!(c.iter().any(|t| t.naive_local() == resolved), "gap resolved to 01:00");
        assert!(c.windows(2).all(|w| w[0] < w[1]), "sorted and deduplicated");
    }

    /// A schedule boundary inside the Berlin autumn fold (02:30 twice).
    #[test]
    fn flip_candidates_survive_fold_boundary() {
        use chrono_tz::Europe::Berlin;
        let mut days = default_week_days();
        days[6].enabled = true; // Sunday 2027-10-31
        days[6].start = NaiveTime::from_hms_opt(2, 30, 0).unwrap();
        let cfg = cfg_with_days(days);
        let start = NaiveDate::from_ymd_opt(2027, 10, 31).unwrap();

        let c = flip_candidates(&Berlin, &cfg.schedule, start, 0, &NoHolidays);

        let fold = start.and_hms_opt(2, 30, 0).unwrap();
        let hit = c.iter().find(|t| t.naive_local() == fold).expect("fold boundary present");
        assert_eq!(hit.naive_utc(), start.and_hms_opt(0, 30, 0).unwrap(), "earlier instant");
    }
}
