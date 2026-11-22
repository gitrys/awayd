//! SPDX-License-Identifier: GPL-3.0-or-later
//! Session debug logging (NDJSON append). Remove after debug session.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SESSION_ID: &str = "f56ff0";
const LOG_NAME: &str = "debug-f56ff0.log";

fn log_path() -> PathBuf {
    if let Ok(p) = std::env::var("AWAYD_AGENT_DEBUG_LOG") {
        return PathBuf::from(p);
    }
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|x| x.to_path_buf())) {
        let from_portable = exe.join("../../..").join(LOG_NAME);
        if from_portable.parent().is_some() {
            return from_portable;
        }
    }
    PathBuf::from(LOG_NAME)
}

/// Append one NDJSON debug line. Never panics; ignores I/O errors.
pub fn log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = serde_json::json!({
        "sessionId": SESSION_ID,
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": ts,
    });
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{line}");
    }
}
