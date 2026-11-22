//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tracing init: stderr for CLI/settings; rotating NDJSON file for daemon.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use chrono::Local;
use parking_lot::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, Layer, Registry};

use crate::config::paths;
use crate::config::schema::AdvancedSection;
use crate::log::{
    capture_level_filter_spec, parse_capture_level, redact_message, LOG_SCHEMA_VERSION,
};

/// CLI / settings: stderr only.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Resident daemon: rotating NDJSON file + optional stderr mirror.
pub fn init_daemon_logging(advanced: &AdvancedSection) -> anyhow::Result<()> {
    paths::ensure_dirs()?;
    let log_dir = paths::logs_dir();
    std::fs::create_dir_all(&log_dir)?;

    let writer = Arc::new(Mutex::new(RotatingWriter::open(
        &log_dir.join("awayd.log"),
        advanced.log_max_bytes,
        advanced.log_max_files,
    )?));

    let file_layer = NdjsonLayer {
        writer: writer.clone(),
    };

    let spec = if rust_log_set() {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| default_capture_filter(advanced))
    } else {
        default_capture_filter(advanced)
    };
    // Global filter as the first layer, behind a reload handle so `log_min_level` changes
    // take effect without restarting the daemon.
    let (filter, handle) = reload::Layer::new(spec);

    if stderr_mirror_enabled() {
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .try_init()?;
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .try_init()?;
    }
    let _ = FILTER_HANDLE.set(handle);

    Ok(())
}

static FILTER_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

fn rust_log_set() -> bool {
    std::env::var("RUST_LOG").is_ok()
}

/// Apply `log_min_level` from a changed config to the running daemon. `RUST_LOG` keeps
/// priority (as at start-up).
pub fn apply_capture_level(advanced: &AdvancedSection) {
    if rust_log_set() {
        tracing::info!(
            target: "awayd::logging",
            log_min_level = %advanced.log_min_level,
            "RUST_LOG is set and overrides log_min_level"
        );
        return;
    }
    let Some(handle) = FILTER_HANDLE.get() else {
        return;
    };
    match handle.reload(default_capture_filter(advanced)) {
        Ok(()) => tracing::info!(
            target: "awayd::logging",
            log_min_level = %advanced.log_min_level,
            "log capture level applied"
        ),
        Err(e) => tracing::warn!(
            target: "awayd::logging",
            error = %e,
            "log capture level not applied"
        ),
    }
}

fn default_capture_filter(advanced: &AdvancedSection) -> EnvFilter {
    let level = parse_capture_level(&advanced.log_min_level);
    EnvFilter::try_new(capture_level_filter_spec(level)).unwrap_or_else(|_| EnvFilter::new("debug"))
}

fn stderr_mirror_enabled() -> bool {
    std::env::var("AWAYD_LOG_STDERR")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

struct RotatingWriter {
    path: std::path::PathBuf,
    max_bytes: u64,
    max_files: usize,
    file: File,
    size: u64,
}

impl RotatingWriter {
    fn open(path: &Path, max_bytes: u64, max_files: usize) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path: path.to_path_buf(),
            max_bytes: max_bytes.max(1024),
            max_files: max_files.max(1),
            file,
            size,
        })
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        let _ = self.file.flush();
        let parent = self.path.parent().unwrap_or(Path::new("."));
        let base = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("awayd.log");
        for i in (1..self.max_files).rev() {
            let from = parent.join(format!("{base}.{i}"));
            let to = parent.join(format!("{base}.{}", i + 1));
            if from.is_file() {
                let _ = std::fs::remove_file(&to);
                let _ = std::fs::rename(&from, &to);
            }
        }
        let first = parent.join(format!("{base}.1"));
        let _ = std::fs::remove_file(&first);
        if self.path.is_file() {
            std::fs::rename(&self.path, &first)?;
        }
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.size = 0;
        Ok(())
    }

    fn write_line(&mut self, line: &[u8]) -> std::io::Result<()> {
        if self.size + line.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(line)?;
        self.file.write_all(b"\n")?;
        self.size += line.len() as u64 + 1;
        Ok(())
    }
}

struct NdjsonLayer {
    writer: Arc<Mutex<RotatingWriter>>,
}

impl<S> Layer<S> for NdjsonLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let level = meta.level().to_string().to_ascii_uppercase();
        let target = meta.target().to_string();
        let message = redact_message(&visitor.message);

        let mut fields = visitor.fields;
        fields.remove("message");
        redact_fields(&mut fields);

        let record = serde_json::json!({
            "v": LOG_SCHEMA_VERSION,
            "ts": log_timestamp(Local::now()),
            "level": level,
            "target": target,
            "message": message,
            "fields": fields,
        });

        if let Ok(line) = serde_json::to_vec(&record) {
            let _ = self.writer.lock().write_line(&line);
        }
    }
}

/// RFC 3339 with milliseconds and UTC offset: unambiguous in the repeated hour when daylight
/// saving time ends, and comparable across time zones (logs posted in issues).
fn log_timestamp(now: chrono::DateTime<Local>) -> String {
    now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
}

/// Field names whose value is a secret as a whole.
fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k == "token"
        || k == "secret"
        || k == "password"
        || k.ends_with("_token")
        || k.ends_with("_secret")
}

/// Structured fields (`error = %e`, `?e`, …) can carry foreign text such as HTTP responses:
/// apply the same redaction as for the message, and drop values of secret-named fields.
fn redact_fields(fields: &mut BTreeMap<String, serde_json::Value>) {
    for (key, value) in fields.iter_mut() {
        if is_secret_key(key) {
            *value = serde_json::Value::String("<redacted>".into());
        } else if let serde_json::Value::String(s) = value {
            *s = redact_message(s);
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: BTreeMap<String, serde_json::Value>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let key = field.name();
        if key == "message" {
            self.message = format!("{value:?}");
            if self.message.starts_with('"')
                && self.message.ends_with('"')
                && self.message.len() >= 2
            {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        } else {
            self.fields
                .insert(key.to_string(), serde_json::json!(format!("{value:?}")));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let key = field.name();
        if key == "message" {
            self.message = value.to_string();
        } else {
            self.fields.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::json!(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn fields_are_redacted_like_the_message() {
        let mut fields = BTreeMap::from([
            (
                "error".to_string(),
                serde_json::json!("Authorization: Bearer FIELDSECRET_1234567890"),
            ),
            ("discord_token".to_string(), serde_json::json!("abc")),
            ("credential".to_string(), serde_json::json!("discord")),
            ("retry_secs".to_string(), serde_json::json!(30)),
        ]);
        redact_fields(&mut fields);
        let text = serde_json::to_string(&fields).unwrap();
        assert!(!text.contains("FIELDSECRET"), "{text}");
        assert_eq!(fields["discord_token"], "<redacted>");
        // Ids and numbers stay readable.
        assert_eq!(fields["credential"], "discord");
        assert_eq!(fields["retry_secs"], 30);
    }

    #[test]
    fn timestamp_has_offset() {
        let t = Local
            .with_ymd_and_hms(2026, 10, 25, 2, 30, 0)
            .earliest()
            .unwrap();
        let ts = log_timestamp(t);
        assert!(ts.starts_with("2026-10-25T02:30:00.000"), "{ts}");
        assert!(
            chrono::DateTime::parse_from_rfc3339(&ts).is_ok(),
            "not RFC 3339: {ts}"
        );
    }
}
