//! SPDX-License-Identifier: GPL-3.0-or-later
//! Parse human-friendly override TTL strings (`1h30m`, `90m`, plain seconds).

pub fn parse_ttl_seconds(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty TTL".into());
    }
    if s.eq_ignore_ascii_case("boundary") {
        return Ok(0);
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return s
            .parse::<u32>()
            .map_err(|_| format!("invalid TTL seconds: {s:?}"));
    }
    let mut total: u64 = 0;
    let mut num = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
        } else {
            let n: u64 = num
                .parse()
                .map_err(|_| format!("invalid TTL fragment in {s:?}"))?;
            num.clear();
            total = total.saturating_add(match ch {
                's' | 'S' => n,
                'm' | 'M' => n.saturating_mul(60),
                'h' | 'H' => n.saturating_mul(3600),
                'd' | 'D' => n.saturating_mul(86_400),
                _ => return Err(format!("unknown TTL unit {ch:?} in {s:?}")),
            });
        }
    }
    // A number without unit after a unit is ambiguous ("1h30" read as 1 h 30 s instead of the
    // intended 1 h 30 min): reject it. A number alone (handled above) stays seconds.
    if !num.is_empty() {
        return Err(format!(
            "missing unit after {num} in {s:?} (s, m, h or d, e.g. \"{s}m\")"
        ));
    }
    u32::try_from(total).map_err(|_| format!("TTL too large: {s:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_plain_seconds() {
        assert_eq!(parse_ttl_seconds("90"), Ok(90));
        assert_eq!(parse_ttl_seconds("90m"), Ok(5400));
        assert_eq!(parse_ttl_seconds("1h30m"), Ok(5400));
        assert_eq!(parse_ttl_seconds("2h"), Ok(7200));
        assert_eq!(parse_ttl_seconds("1d"), Ok(86_400));
        assert_eq!(parse_ttl_seconds("0"), Ok(0));
        assert_eq!(parse_ttl_seconds("boundary"), Ok(0));
    }

    #[test]
    fn number_without_unit_after_a_unit_is_rejected() {
        let err = parse_ttl_seconds("1h30").unwrap_err();
        assert!(err.contains("missing unit after 30"), "{err}");
        assert!(err.contains("1h30m"), "suggests the fix: {err}");
    }

    #[test]
    fn invalid_input_is_rejected() {
        assert!(parse_ttl_seconds("").is_err());
        assert!(parse_ttl_seconds("5x").is_err());
        assert!(parse_ttl_seconds("h").is_err());
    }
}
