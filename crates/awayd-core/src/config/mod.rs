//! SPDX-License-Identifier: GPL-3.0-or-later
//! Load / save `config.toml` and `state.json`.

pub mod paths;
pub mod schema;
pub mod state;

use crate::atomic_fs;
use crate::schedule;
use anyhow::{Context, Result};
use schema::{AppConfig, CONFIG_SCHEMA_VERSION};
use state::AppState;
use std::path::Path;

pub fn load_config(path: &Path) -> Result<AppConfig> {
    if !path.is_file() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    parse_config(&raw)
}

/// Parse and validate `config.toml` text (same rules as [`load_config`]).
pub fn parse_config(raw: &str) -> Result<AppConfig> {
    let mut c: AppConfig = toml::from_str(raw).with_context(|| "parse config.toml")?;
    schedule::normalize_schedule(&mut c.schedule);
    if c.schema_version != CONFIG_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported config schema_version {} (expected {})",
            c.schema_version,
            CONFIG_SCHEMA_VERSION
        );
    }
    Ok(c)
}

/// Parse vacation entries into inclusive date ranges; rejects invalid dates and start > end.
pub fn vacation_ranges_from_entries(
    entries: &[schema::VacationEntry],
) -> std::result::Result<Vec<(chrono::NaiveDate, chrono::NaiveDate)>, String> {
    let mut out = Vec::new();
    for e in entries {
        let start = chrono::NaiveDate::parse_from_str(&e.start, "%Y-%m-%d")
            .map_err(|err| format!("vacation {}: start {:?}: {err}", e.id, e.start))?;
        let end = chrono::NaiveDate::parse_from_str(&e.end, "%Y-%m-%d")
            .map_err(|err| format!("vacation {}: end {:?}: {err}", e.id, e.end))?;
        if start > end {
            return Err(format!("vacation {}: start after end", e.id));
        }
        out.push((start, end));
    }
    Ok(out)
}

/// The one validation for a config the daemon will apply: used by the daemon (tick and
/// `set_config_toml`), `awayctl validate` and the settings window before saving, so all of
/// them accept and reject the same files.
pub fn validate_app_config(app: &AppConfig) -> std::result::Result<(), String> {
    let mut sched = app.schedule.clone();
    schedule::normalize_schedule(&mut sched);
    schedule::validate_schedule(&sched)?;
    vacation_ranges_from_entries(&app.schedule.vacations)?;
    validate_backend_ids(app)?;
    validate_mode(&app.mode)?;
    Ok(())
}

/// Longest override "until the next change" in network mode.
const MAX_NETWORK_OVERRIDE_HOURS: f64 = 7.0 * 24.0;

fn validate_mode(mode: &schema::ModeSection) -> std::result::Result<(), String> {
    if mode.probe_interval_seconds < schema::MIN_PROBE_INTERVAL_SECONDS {
        return Err(format!(
            "mode.probe_interval_seconds must be at least {} (is {})",
            schema::MIN_PROBE_INTERVAL_SECONDS,
            mode.probe_interval_seconds
        ));
    }
    let ttl = mode.network_override_ttl_hours;
    if !(ttl > 0.0 && ttl <= MAX_NETWORK_OVERRIDE_HOURS) {
        return Err(format!(
            "mode.network_override_ttl_hours must be more than 0 and at most {MAX_NETWORK_OVERRIDE_HOURS} (is {ttl})"
        ));
    }
    if mode.kind == "network" && mode.work_host.trim().is_empty() {
        return Err("mode.work_host is required for kind = \"network\"".into());
    }
    Ok(())
}

/// A typo in `[backends.<id>]` must not silently leave the real service switched on.
fn validate_backend_ids(app: &AppConfig) -> std::result::Result<(), String> {
    for id in app.backends.keys() {
        if crate::backends::find(id).is_none() {
            let known: Vec<&str> = crate::backends::ALL.iter().map(|b| b.id).collect();
            return Err(format!(
                "unknown service [backends.{id}] (known: {})",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

pub fn save_config(path: &Path, cfg: &AppConfig) -> Result<()> {
    let raw = toml::to_string_pretty(cfg).context("serialize config")?;
    atomic_fs::atomic_write(path, raw.as_bytes())?;
    Ok(())
}

/// Load `state.json`. It only holds runtime data (override, last apply): when it cannot be
/// parsed or has another schema version (e.g. written by a newer awayd), it is moved aside as
/// `state.json.<time>.bak` and a fresh state is used. Before, every tick failed until the file
/// was removed by hand. A file that cannot be read at all (locked, no access) stays an error.
pub fn load_state(path: &Path) -> Result<AppState> {
    if !path.is_file() {
        return Ok(AppState::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    match parse_state(&raw) {
        Ok(st) => Ok(st),
        Err(reason) => {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "state.json".into());
            let backup = path.with_file_name(format!("{file_name}.{stamp}.bak"));
            std::fs::rename(path, &backup)
                .with_context(|| format!("move unusable {} aside", path.display()))?;
            tracing::warn!(
                target: "awayd::state",
                backup = %backup.display(),
                "{reason}; moved aside, starting with a fresh state"
            );
            Ok(AppState::default())
        }
    }
}

fn parse_state(raw: &str) -> std::result::Result<AppState, String> {
    let st: AppState =
        serde_json::from_str(raw).map_err(|e| format!("state.json unreadable: {e}"))?;
    if st.schema_version != state::STATE_SCHEMA_VERSION {
        return Err(format!(
            "state.json has schema_version {} (expected {})",
            st.schema_version,
            state::STATE_SCHEMA_VERSION
        ));
    }
    Ok(st)
}

pub fn save_state(path: &Path, st: &AppState) -> Result<()> {
    let raw = serde_json::to_string_pretty(st)?;
    atomic_fs::atomic_write(path, raw.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::VacationEntry;

    fn with_vacation(start: &str, end: &str) -> AppConfig {
        let mut c = AppConfig::default();
        c.schedule.vacations = vec![VacationEntry {
            id: "v0".into(),
            label: "Test".into(),
            start: start.into(),
            end: end.into(),
            comment: String::new(),
        }];
        c
    }

    #[test]
    fn default_config_is_valid() {
        assert_eq!(validate_app_config(&AppConfig::default()), Ok(()));
        assert_eq!(
            validate_app_config(&with_vacation("2026-12-20", "2026-12-28")),
            Ok(())
        );
    }

    /// `awayctl validate` used to accept this while the daemon rejected it.
    #[test]
    fn reversed_vacation_is_rejected() {
        let err = validate_app_config(&with_vacation("2026-12-28", "2026-12-20")).unwrap_err();
        assert!(err.contains("start after end"), "{err}");
    }

    #[test]
    fn invalid_vacation_date_is_rejected() {
        let err = validate_app_config(&with_vacation("2026-13-01", "2026-12-28")).unwrap_err();
        assert!(err.contains("v0") && err.contains("2026-13-01"), "{err}");
    }

    /// Configuration written by 0.1.0: empty `[discord]`, no `[backends]`.
    const CONFIG_0_1_0: &str =
        "schema_version = 1\n\n[discord]\n\n[schedule]\ncountry = \"DE\"\nsubdiv = \"BW\"\n";

    #[test]
    fn config_without_backends_section_keeps_all_services_on() {
        let c = parse_config(CONFIG_0_1_0).unwrap();
        assert!(c.backends.is_empty());
        for b in crate::backends::ALL {
            assert!(c.backend_enabled(b.id), "{}", b.id);
        }
        assert_eq!(validate_app_config(&c), Ok(()));
        let saved = toml::to_string_pretty(&c).unwrap();
        assert!(!saved.contains("[discord]"), "{saved}");
    }

    #[test]
    fn new_config_has_discord_off_and_steam_on() {
        let c = AppConfig::default();
        assert!(!c.backend_enabled(crate::backends::DISCORD));
        assert!(c.backend_enabled(crate::backends::STEAM));

        let saved = toml::to_string_pretty(&c).unwrap();
        assert!(saved.contains("[backends.discord]"), "{saved}");
        let back = parse_config(&saved).unwrap();
        assert!(!back.backend_enabled(crate::backends::DISCORD));
        assert!(back.backend_enabled(crate::backends::STEAM));
    }

    #[test]
    fn switching_a_service_roundtrips() {
        let mut c = parse_config(CONFIG_0_1_0).unwrap();
        c.set_backend_enabled(crate::backends::STEAM, false);
        let back = parse_config(&toml::to_string_pretty(&c).unwrap()).unwrap();
        assert!(!back.backend_enabled(crate::backends::STEAM));
        assert!(back.backend_enabled(crate::backends::DISCORD));
    }

    #[test]
    fn missing_enabled_key_means_on() {
        let raw = format!("{CONFIG_0_1_0}\n[backends.steam]\n");
        assert!(parse_config(&raw)
            .unwrap()
            .backend_enabled(crate::backends::STEAM));
    }

    #[test]
    fn unknown_service_is_rejected() {
        let raw = format!("{CONFIG_0_1_0}\n[backends.discrod]\nenabled = false\n");
        let err = validate_app_config(&parse_config(&raw).unwrap()).unwrap_err();
        assert!(
            err.contains("discrod") && err.contains("discord, steam"),
            "{err}"
        );
    }

    #[test]
    fn unusable_state_is_moved_aside_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        for content in [
            "{ not json",
            r#"{"schema_version": 99}"#, // e.g. written by a newer awayd
        ] {
            std::fs::write(&path, content).unwrap();
            let st = load_state(&path).expect("fresh state instead of an error");
            assert_eq!(st.schema_version, state::STATE_SCHEMA_VERSION);
            assert!(!path.exists(), "unusable file moved away");
            let backups: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
                .collect();
            assert!(!backups.is_empty());
            for b in backups {
                assert_eq!(std::fs::read_to_string(b.path()).unwrap(), content);
                std::fs::remove_file(b.path()).unwrap();
            }
        }
    }

    #[test]
    fn valid_state_is_loaded_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        save_state(&path, &AppState::default()).unwrap();
        load_state(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn network_mode_settings_are_validated() {
        let mut c = AppConfig::default();
        assert_eq!(validate_app_config(&c), Ok(()));

        c.mode.kind = "network".into();
        let err = validate_app_config(&c).unwrap_err();
        assert!(err.contains("work_host"), "{err}");

        c.mode.work_host = "192.0.2.10".into();
        c.mode.probe = schema::ProbeKind::Ping;
        assert_eq!(validate_app_config(&c), Ok(()));

        c.mode.probe_interval_seconds = 5;
        assert!(validate_app_config(&c)
            .unwrap_err()
            .contains("probe_interval_seconds"));
        c.mode.probe_interval_seconds = 60;

        for bad in [0.0, -1.0, 200.0, f64::NAN] {
            c.mode.network_override_ttl_hours = bad;
            assert!(
                validate_app_config(&c)
                    .unwrap_err()
                    .contains("network_override_ttl_hours"),
                "{bad}"
            );
        }
    }

    #[test]
    fn probe_kind_reads_from_toml_and_defaults_to_tcp() {
        let c = parse_config(CONFIG_0_1_0).unwrap();
        assert_eq!(c.mode.probe, schema::ProbeKind::Tcp);
        assert_eq!(c.mode.probe_interval_seconds, 60);

        let raw = format!(
            "{CONFIG_0_1_0}\n[mode]\nkind = \"network\"\nwork_host = \"192.0.2.10\"\nprobe = \"ping\"\n"
        );
        let c = parse_config(&raw).unwrap();
        assert_eq!(c.mode.probe, schema::ProbeKind::Ping);
        assert_eq!(validate_app_config(&c), Ok(()));
    }
}
