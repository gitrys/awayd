//! SPDX-License-Identifier: GPL-3.0-or-later
//! NDJSON log entries, redaction, and display filters (capture vs viewer).

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::Level;

pub const LOG_SCHEMA_VERSION: u32 = 1;

/// UI grouping for display-only filters (file stores real `level`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBucket {
    Verbose,
    Warning,
    Errors,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub v: u32,
    pub ts: String,
    pub level: String,
    pub target: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, serde_json::Value>,
}

impl LogEntry {
    pub fn level_bucket(&self) -> DisplayBucket {
        level_str_bucket(&self.level)
    }

    pub fn haystack(&self) -> String {
        let mut s = format!("{} {} {}", self.level, self.target, self.message);
        if !self.fields.is_empty() {
            if let Ok(j) = serde_json::to_string(&self.fields) {
                s.push(' ');
                s.push_str(&j);
            }
        }
        s
    }
}

pub fn level_str_bucket(level: &str) -> DisplayBucket {
    match level.to_ascii_uppercase().as_str() {
        "WARN" | "WARNING" => DisplayBucket::Warning,
        "ERROR" => DisplayBucket::Errors,
        _ => DisplayBucket::Verbose,
    }
}

pub fn tracing_level_bucket(level: Level) -> DisplayBucket {
    match level {
        Level::WARN => DisplayBucket::Warning,
        Level::ERROR => DisplayBucket::Errors,
        _ => DisplayBucket::Verbose,
    }
}

/// Display-only filter (does not affect capture).
pub fn matches_display_filter(
    entry: &LogEntry,
    show_verbose: bool,
    show_warning: bool,
    show_errors: bool,
    search: &str,
) -> bool {
    let bucket = entry.level_bucket();
    let level_ok = match bucket {
        DisplayBucket::Verbose => show_verbose,
        DisplayBucket::Warning => show_warning,
        DisplayBucket::Errors => show_errors,
    };
    if !level_ok {
        return false;
    }
    let q = search.trim();
    if q.is_empty() {
        return true;
    }
    entry
        .haystack()
        .to_ascii_lowercase()
        .contains(&q.to_ascii_lowercase())
}

pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    serde_json::from_str(line).ok()
}

pub fn parse_log_bytes(chunk: &[u8]) -> Vec<LogEntry> {
    let mut out = Vec::new();
    for line in chunk.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(line) {
            if let Some(e) = parse_log_line(s) {
                out.push(e);
            }
        }
    }
    out
}

static REDACT_PATTERNS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();

fn redact_patterns() -> &'static Vec<Regex> {
    REDACT_PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"(?i)(authorization:\s*bearer\s+)(\S+)").unwrap(),
            Regex::new(r"(?i)(bearer\s+)(\S+)").unwrap(),
            // Credential keys of any backend (`discord_token=…`, `slack_credential: …`).
            Regex::new(r"(?i)(\b\w+_(?:token|credential|secret)\s*[=:]\s*)(\S+)").unwrap(),
            Regex::new(r"(?i)(token\s*[=:]\s*)([A-Za-z0-9._\-]{20,})").unwrap(),
            // Raw Discord user token without any key (three base64url parts).
            Regex::new(r"()([\w-]{24,}\.[\w-]{6}\.[\w-]{27,})").unwrap(),
        ]
    })
}

/// Redact secrets before persisting log lines.
pub fn redact_message(s: &str) -> String {
    let mut out = s.to_string();
    for re in redact_patterns() {
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                format!("{}{}", &caps[1], "<redacted>")
            })
            .into_owned();
    }
    out
}

pub fn parse_capture_level(s: &str) -> Level {
    match s.trim().to_ascii_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::DEBUG,
    }
}

pub fn capture_level_filter_spec(level: Level) -> String {
    format!("{}", level_as_filter_str(level))
}

fn level_as_filter_str(level: Level) -> &'static str {
    match level {
        Level::TRACE => "trace",
        Level::DEBUG => "debug",
        Level::INFO => "info",
        Level::WARN => "warn",
        Level::ERROR => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entry_roundtrip() {
        let raw = r#"{"v":1,"ts":"2026-06-12T08:04:47.123","level":"INFO","target":"awayd::apply","message":"presence sent","fields":{"backend":"discord"}}"#;
        let e = parse_log_line(raw).unwrap();
        assert_eq!(e.message, "presence sent");
        assert_eq!(e.fields.get("backend").unwrap(), "discord");
    }

    #[test]
    fn redact_bearer() {
        let s = "Authorization: Bearer abc.def.ghijklmnop";
        assert!(redact_message(s).contains("<redacted>"));
        assert!(!redact_message(s).contains("ghijklmnop"));
    }

    #[test]
    fn redact_credential_keys_of_any_backend() {
        for s in [
            "discord_token=abc123",
            "slack_credential: xyz",
            "teams_secret = s3cr3t",
        ] {
            let r = redact_message(s);
            assert!(r.ends_with("<redacted>"), "{s} -> {r}");
        }
        // Made-up value in Discord's token format (no key, no "Bearer"), built at runtime so
        // no token-shaped literal is in the source (secret scanners flag it).
        let fake = format!("{}.{}.{}", "x".repeat(26), "y".repeat(6), "z".repeat(30));
        let raw = format!("msg C raw token {fake}");
        assert_eq!(redact_message(&raw), "msg C raw token <redacted>");
        // Plain words are kept: status texts mention "token" without a value.
        assert_eq!(
            redact_message("Discord token missing"),
            "Discord token missing"
        );
    }

    #[test]
    fn display_filter_verbose_only() {
        let e = LogEntry {
            v: 1,
            ts: String::new(),
            level: "DEBUG".into(),
            target: "t".into(),
            message: "hello".into(),
            fields: BTreeMap::new(),
        };
        assert!(matches_display_filter(&e, true, false, false, ""));
        assert!(!matches_display_filter(&e, false, true, true, ""));
    }

    #[test]
    fn search_matches_fields() {
        let mut fields = BTreeMap::new();
        fields.insert("backend".into(), serde_json::json!("steam"));
        let e = LogEntry {
            v: 1,
            ts: String::new(),
            level: "INFO".into(),
            target: "awayd::apply".into(),
            message: "presence sent".into(),
            fields,
        };
        assert!(matches_display_filter(&e, true, true, true, "steam"));
        assert!(!matches_display_filter(&e, true, true, true, "discord"));
    }
}
