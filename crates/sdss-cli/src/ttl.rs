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
    if !num.is_empty() {
        let n: u64 = num
            .parse()
            .map_err(|_| format!("invalid TTL fragment in {s:?}"))?;
        total = total.saturating_add(n);
    }
    u32::try_from(total).map_err(|_| format!("TTL too large: {s:?}"))
}
