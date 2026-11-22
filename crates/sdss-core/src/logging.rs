//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tracing init: stderr for CLI/settings; rotating NDJSON file for daemon.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use chrono::Local;
use parking_lot::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

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
        &log_dir.join("sdss.log"),
        advanced.log_max_bytes,
        advanced.log_max_files,
    )?));

    let file_layer = NdjsonLayer {
        writer: writer.clone(),
    };

    let spec = if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| default_capture_filter(advanced))
    } else {
        default_capture_filter(advanced)
    };

    if stderr_mirror_enabled() {
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        tracing_subscriber::registry()
            .with(file_layer)
            .with(stderr_layer)
            .with(spec)
            .try_init()?;
    } else {
        tracing_subscriber::registry()
            .with(file_layer)
            .with(spec)
            .try_init()?;
    }

    Ok(())
}

/// Rebuild capture filter when config changes (best-effort; subscriber already init).
pub fn apply_capture_level(advanced: &AdvancedSection) {
    let level = parse_capture_level(&advanced.log_min_level);
    tracing::info!(
        target: "sdss::logging",
        log_min_level = %advanced.log_min_level,
        "log capture level configured (restart daemon to change filter if RUST_LOG unset)"
    );
    let _ = level;
}

fn default_capture_filter(advanced: &AdvancedSection) -> EnvFilter {
    let level = parse_capture_level(&advanced.log_min_level);
    EnvFilter::try_new(capture_level_filter_spec(level)).unwrap_or_else(|_| EnvFilter::new("debug"))
}

fn stderr_mirror_enabled() -> bool {
    std::env::var("SDSS_LOG_STDERR")
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
            .unwrap_or("sdss.log");
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

        let record = serde_json::json!({
            "v": LOG_SCHEMA_VERSION,
            "ts": Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
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
