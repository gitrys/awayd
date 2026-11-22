//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use awayd_core::backends;
use awayd_core::config::paths;
use awayd_core::config::schema::{AppConfig, DayWindow};
use awayd_core::config::{self};
use awayd_core::schedule::{schedule_days, DAY_LABELS};
use awayd_core::status::{BackendApplyRecord, BackendApplyStatus, StatusSnapshot};
use awayd_ipc::SchedulerClient;

fn is_tty() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}

fn c(code: &str, s: &str) -> String {
    if is_tty() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn format_day_line(day: &DayWindow, label: &str) -> String {
    if day.enabled {
        format!(
            "  {:<4} {}–{}",
            label,
            day.start.format("%H:%M"),
            day.end.format("%H:%M")
        )
    } else {
        format!("  {:<4} off", label)
    }
}

fn print_schedule_days(days: &[DayWindow; 7]) {
    println!("  schedule days (Mon..Sun):");
    for (i, day) in days.iter().enumerate() {
        println!("{}", format_day_line(day, DAY_LABELS[i]));
    }
}

/// "discord off, steam on".
fn services_line(cfg: &AppConfig) -> String {
    backends::ALL
        .iter()
        .map(|b| {
            let state = if cfg.backend_enabled(b.id) {
                "on"
            } else {
                "off"
            };
            format!("{} {state}", b.id)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// "sent", "skipped (steam is not running)", "failed (401 unauthorized)".
fn backend_line(b: &BackendApplyRecord) -> String {
    let status = match b.status {
        BackendApplyStatus::Sent => "sent",
        BackendApplyStatus::Skipped => "skipped",
        BackendApplyStatus::Failed => "failed",
    };
    match &b.message {
        Some(m) => format!("{status} ({m})"),
        None => status.to_string(),
    }
}

fn print_pretty_table(s: &StatusSnapshot) {
    let mode = if s.work_mode {
        "invisible (work)"
    } else {
        "online (off)"
    };
    println!("{}", c("1", "awayd doctor — daemon status"));
    let daemon_version = if s.version.is_empty() {
        "unknown (older daemon)"
    } else {
        s.version.as_str()
    };
    println!("  {:<16} {}", "daemon version:", daemon_version);
    println!(
        "  {:<16} {}",
        "work_mode:",
        c(if s.work_mode { "31" } else { "32" }, mode)
    );
    println!("  {:<16} {}", "source:", s.source);
    println!("  {:<16} {}", "mode_kind:", s.mode_kind);
    println!("  {:<16} {}", "ok:", if s.ok { "yes" } else { "no" });
    if let Some(ref nw) = s.next_wake {
        println!("  {:<16} {}", "next_wake:", nw);
    }
    if let Some(ref la) = s.last_apply {
        println!("  {:<16} {}", "last_apply.at:", la.at);
        println!(
            "  {:<16} {}",
            "last_apply.ok:",
            if la.ok { "yes" } else { "no" }
        );
        for b in &la.backends {
            println!("  {:<16} {}", format!("{}:", b.id), backend_line(b));
        }
    }
    if !s.errors.is_empty() {
        println!("  errors:");
        for e in &s.errors {
            println!("    - {e}");
        }
    }
}

/// Exit code 0 only when the daemon answers and reports `ok`; otherwise `main` prints the
/// error on stderr and exits with 1 (stdout stays plain JSON with `--json`).
pub async fn doctor(json: bool, pretty: bool) -> Result<()> {
    let cfg_loaded = config::load_config(&paths::config_file());

    let mut client = SchedulerClient::new();
    let status_raw = if client.ping().await.unwrap_or(false) {
        match client.status().await {
            Ok(raw) => Some(raw),
            Err(e) => {
                tracing::warn!(target: "awayd::doctor", "status failed: {e}");
                None
            }
        }
    } else {
        None
    };

    if json {
        let status = status_raw
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
        println!("{}", doctor_json(status, cfg_loaded.as_ref().ok()));
        return outcome(status_raw.as_deref());
    }

    println!("  {:<16} {}", "awayctl:", awayd_core::VERSION);
    println!("  {:<16} {}", "portable:", paths::is_portable());
    println!("  {:<16} {}", "config:", paths::config_file().display());
    println!("  {:<16} {}", "state:", paths::state_file().display());

    match status_raw.as_deref() {
        Some(raw) => match serde_json::from_str::<StatusSnapshot>(raw) {
            Ok(s) if pretty => {
                print_pretty_table(&s);
                if let Ok(cfg) = &cfg_loaded {
                    println!("  {:<16} {}", "services:", services_line(cfg));
                    let cc = cfg.schedule.country.to_uppercase();
                    if let Ok((ready, y0, y1, fetched, msg)) = client.holiday_data_status(&cc).await
                    {
                        println!(
                            "  {:<16} {}",
                            "holidays:",
                            if ready {
                                format!("{cc} {y0}..{y1} (updated {fetched})")
                            } else {
                                msg
                            }
                        );
                    }
                }
            }
            _ => println!("daemon status:\n{raw}"),
        },
        None => {
            let cfg = cfg_loaded?;
            println!("{}", c("33", "no daemon; local config:"));
            println!(
                "  country={} subdiv={}",
                cfg.schedule.country, cfg.schedule.subdiv
            );
            println!("  services: {}", services_line(&cfg));
            print_schedule_days(&schedule_days(&cfg.schedule));
        }
    }
    outcome(status_raw.as_deref())
}

/// `Err` (exit code 1) when the daemon is not reachable or reports problems.
fn outcome(status_raw: Option<&str>) -> Result<()> {
    match status_raw.map(serde_json::from_str::<StatusSnapshot>) {
        None => anyhow::bail!("awayd is not running or not reachable"),
        Some(Ok(s)) if s.ok => Ok(()),
        Some(Ok(_)) => anyhow::bail!("awayd reports problems (ok: no, see errors)"),
        Some(Err(e)) => anyhow::bail!("unreadable status from awayd: {e}"),
    }
}

/// One JSON object for `doctor --json`: paths, the daemon status (`null` without daemon) and
/// the local configuration (`null` when config.toml cannot be read).
fn doctor_json(status: Option<serde_json::Value>, cfg: Option<&AppConfig>) -> serde_json::Value {
    let local = cfg.map(|c| {
        let services: serde_json::Map<String, serde_json::Value> = backends::ALL
            .iter()
            .map(|b| (b.id.to_string(), c.backend_enabled(b.id).into()))
            .collect();
        serde_json::json!({
            "country": c.schedule.country,
            "subdiv": c.schedule.subdiv,
            "services": services,
        })
    });
    serde_json::json!({
        "awayctl": awayd_core::VERSION,
        "portable": paths::is_portable(),
        "config": paths::config_file().display().to_string(),
        "state": paths::state_file().display().to_string(),
        "status": status,
        "local": local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_one_object_with_status_and_local_config() {
        let status = serde_json::json!({"ok": true, "work_mode": false});
        let mut cfg = AppConfig::default();
        cfg.set_backend_enabled(backends::STEAM, false);
        let v = doctor_json(Some(status.clone()), Some(&cfg));
        let text = v.to_string();
        let back: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back["status"], status);
        assert_eq!(back["awayctl"], awayd_core::VERSION);
        assert_eq!(back["local"]["services"]["steam"], false);
        assert_eq!(back["local"]["services"]["discord"], false);

        let none = doctor_json(None, None);
        assert!(none["status"].is_null() && none["local"].is_null());
    }

    #[test]
    fn exit_status_follows_daemon_ok() {
        assert!(outcome(None).is_err());
        assert!(outcome(Some(
            r#"{"ok":true,"schema":1,"work_mode":false,"source":"schedule","mode_kind":"schedule"}"#
        ))
        .is_ok());
        assert!(outcome(Some(r#"{"ok":false,"schema":1,"work_mode":false,"source":"schedule","mode_kind":"schedule"}"#)).is_err());
        assert!(outcome(Some("not json")).is_err());
    }
}
