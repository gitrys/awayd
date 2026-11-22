//! SPDX-License-Identifier: GPL-3.0-or-later

use std::fs::File;
use std::io::{BufRead, BufReader};

use anyhow::Result;
use awayd_core::config::paths;
use awayd_core::log::{matches_display_filter, parse_log_line};

#[derive(Debug, Clone, Copy)]
enum LevelFilter {
    All,
    Verbose,
    Warning,
    Error,
}

pub fn logs(tail: usize, grep: Option<String>, level: Option<String>) -> Result<()> {
    let path = paths::current_log_file();
    if !path.is_file() {
        println!("No log file at {}", path.display());
        return Ok(());
    }

    let level_filter = parse_level_arg(level.as_deref());
    let grep = grep.unwrap_or_default();

    let file = File::open(&path)?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<std::io::Result<_>>()?;

    let slice = if tail > 0 {
        &lines[lines.len().saturating_sub(tail)..]
    } else {
        &lines[..]
    };

    let mut shown = 0usize;
    for line in slice {
        let Some(entry) = parse_log_line(line) else {
            continue;
        };
        let (verbose, warning, errors) = level_filter.buckets();
        if !matches_display_filter(&entry, verbose, warning, errors, &grep) {
            continue;
        }
        println!(
            "{} {:5} {} {}",
            entry.ts, entry.level, entry.target, entry.message
        );
        shown += 1;
    }

    if shown == 0 {
        println!("(no matching log lines)");
    }
    Ok(())
}

fn parse_level_arg(s: Option<&str>) -> LevelFilter {
    match s.map(|x| x.to_ascii_lowercase()).as_deref() {
        Some("verbose" | "debug" | "info") => LevelFilter::Verbose,
        Some("warn" | "warning") => LevelFilter::Warning,
        Some("error") => LevelFilter::Error,
        _ => LevelFilter::All,
    }
}

impl LevelFilter {
    fn buckets(self) -> (bool, bool, bool) {
        match self {
            LevelFilter::All => (true, true, true),
            LevelFilter::Verbose => (true, false, false),
            LevelFilter::Warning => (false, true, false),
            LevelFilter::Error => (false, false, true),
        }
    }
}
