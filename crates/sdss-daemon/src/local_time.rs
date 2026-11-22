//! SPDX-License-Identifier: GPL-3.0-or-later
//! DST-safe helpers for local wall-clock times.
//!
//! `chrono` returns no result for wall-clock times that do not exist (spring gap) and two
//! results for times that occur twice (autumn fold). Unwrapping those panics, and with
//! `panic = "abort"` that ends the whole daemon. These helpers never panic.

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Timelike};

/// Upper bound for skipping forward over a gap; real DST gaps are at most two hours.
const MAX_GAP_MINUTES: i64 = 24 * 60;

/// Truncate to the full minute while keeping the instant's UTC offset.
///
/// Subtracts the elapsed seconds instead of rebuilding the value from its wall-clock time,
/// so it cannot fail inside a repeated hour.
pub fn trim_to_minute<Tz: TimeZone>(dt: DateTime<Tz>) -> DateTime<Tz> {
    let elapsed = Duration::seconds(i64::from(dt.second()))
        + Duration::nanoseconds(i64::from(dt.nanosecond()));
    dt - elapsed
}

/// Map a wall-clock time to an instant in `tz`.
///
/// - exists once: that instant
/// - exists twice (fold): the earlier instant
/// - does not exist (gap): the first valid time after it, stepping in whole minutes
pub fn resolve_local<Tz: TimeZone>(tz: &Tz, naive: NaiveDateTime) -> DateTime<Tz> {
    let mut candidate = naive;
    for _ in 0..=MAX_GAP_MINUTES {
        if let Some(dt) = tz.from_local_datetime(&candidate).earliest() {
            return dt;
        }
        candidate += Duration::minutes(1);
    }
    // Not reachable with real time zone data; interpret as UTC rather than panic.
    tz.from_utc_datetime(&naive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use chrono_tz::America::Santiago;
    use chrono_tz::Europe::Berlin;

    fn naive(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    #[test]
    fn resolve_unique_time_is_unchanged() {
        let n = naive(2027, 10, 30, 12, 0, 0);
        let dt = resolve_local(&Berlin, n);
        assert_eq!(dt.naive_local(), n);
        assert_eq!(dt.naive_utc(), naive(2027, 10, 30, 10, 0, 0));
    }

    #[test]
    fn resolve_fold_takes_earlier_instant() {
        // 2027-10-31 02:00-02:59 occurs twice in Berlin (CEST, then CET).
        let n = naive(2027, 10, 31, 2, 30, 0);
        assert!(Berlin.from_local_datetime(&n).single().is_none(), "precondition: fold");
        let dt = resolve_local(&Berlin, n);
        assert_eq!(dt.naive_local(), n);
        assert_eq!(dt.naive_utc(), naive(2027, 10, 31, 0, 30, 0));
    }

    #[test]
    fn resolve_gap_skips_to_first_valid_minute() {
        // 2027-03-28 02:00-02:59 does not exist in Berlin.
        let n = naive(2027, 3, 28, 2, 30, 0);
        assert!(Berlin.from_local_datetime(&n).earliest().is_none(), "precondition: gap");
        let dt = resolve_local(&Berlin, n);
        assert_eq!(dt.naive_local(), naive(2027, 3, 28, 3, 0, 0));
        assert_eq!(dt.naive_utc(), naive(2027, 3, 28, 1, 0, 0));
    }

    #[test]
    fn resolve_midnight_gap() {
        // Santiago starts DST at midnight: 2027-09-05 00:00 does not exist.
        let n = naive(2027, 9, 5, 0, 0, 0);
        assert!(Santiago.from_local_datetime(&n).earliest().is_none(), "precondition: gap");
        let dt = resolve_local(&Santiago, n);
        assert_eq!(dt.naive_local(), naive(2027, 9, 5, 1, 0, 0));
    }

    #[test]
    fn trim_in_fold_keeps_the_instant_offset() {
        let n = naive(2027, 10, 31, 2, 30, 17);
        let first = Berlin.from_local_datetime(&n).earliest().unwrap();
        let second = Berlin.from_local_datetime(&n).latest().unwrap();
        // The old code (`with_second(0).unwrap()`) panicked on both.
        let t1 = trim_to_minute(first);
        let t2 = trim_to_minute(second);
        assert_eq!(t1.naive_local(), naive(2027, 10, 31, 2, 30, 0));
        assert_eq!(t2.naive_local(), naive(2027, 10, 31, 2, 30, 0));
        assert_eq!(t1.naive_utc(), naive(2027, 10, 31, 0, 30, 0));
        assert_eq!(t2.naive_utc(), naive(2027, 10, 31, 1, 30, 0));
    }

    #[test]
    fn trim_drops_nanoseconds() {
        let dt = Berlin.from_local_datetime(&naive(2027, 1, 4, 9, 15, 42)).unwrap()
            + Duration::nanoseconds(123_456_789);
        let t = trim_to_minute(dt);
        assert_eq!(t.naive_local(), naive(2027, 1, 4, 9, 15, 0));
        assert_eq!(t.nanosecond(), 0);
    }
}
