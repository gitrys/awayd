//! SPDX-License-Identifier: GPL-3.0-or-later
//! Main scheduling loop: next wake deadline, poke channel, coordinated shutdown.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::{DateTime, Datelike, Local};
use sdss_core::status::{LastApplyRecord, StatusSnapshot, STATUS_SCHEMA_VERSION};
use tokio::sync::mpsc;
use tokio::sync::{Notify, RwLock, watch};

use crate::apply::apply_presence;
use crate::apply::ApplyResult;
use crate::holidays::{load_holiday_nearby_years, HolidaySet};
use crate::local_time::{resolve_local, trim_to_minute};
use crate::override_state;
use crate::runtime_config::build_config;
use crate::schedule_next::next_schedule_flip_after_default;
use crate::secret;
use crate::service::StatusCache;
use crate::watchers::PokeGate;
use sdss_core::config::paths;
use sdss_core::config::state::AppState;
use sdss_core::config::{self};

/// Last successfully applied `online` flag per backend (`online` = personal / not work).
#[derive(Debug, Clone, Copy, Default)]
struct BackendApplied {
    discord_online: Option<bool>,
    steam_online: Option<bool>,
}

const APPLY_RETRY_INITIAL: Duration = Duration::from_secs(30);
const APPLY_RETRY_MAX: Duration = Duration::from_secs(120);

fn needs_apply(
    force: bool,
    immediate: bool,
    target_online: bool,
    applied: &BackendApplied,
) -> bool {
    force
        || immediate
        || applied.discord_online != Some(target_online)
        || applied.steam_online != Some(target_online)
}

fn record_apply(applied: &mut BackendApplied, target_online: bool, result: &ApplyResult) {
    if result.discord_ok {
        applied.discord_online = Some(target_online);
    }
    if result.steam_ok {
        applied.steam_online = Some(target_online);
    }
}

fn next_sleep_after_tick(
    schedule_sleep: Duration,
    partial_failure: bool,
    retry_backoff: &mut Duration,
) -> Duration {
    if partial_failure {
        let retry = (*retry_backoff).min(APPLY_RETRY_MAX);
        *retry_backoff = (*retry_backoff * 2).min(APPLY_RETRY_MAX);
        schedule_sleep.min(retry).max(Duration::from_secs(1))
    } else {
        *retry_backoff = APPLY_RETRY_INITIAL;
        schedule_sleep
    }
}

fn next_wake_time(
    now: DateTime<Local>,
    cfg: &crate::work_window::Config,
    hol: &HolidaySet,
    st: &AppState,
) -> DateTime<Local> {
    let now = trim_to_minute(now);
    let mut candidates: Vec<DateTime<Local>> = Vec::new();
    if let Ok(Some(ov)) = override_state::override_from_state(st) {
        let exp = resolve_local(&Local, ov.expires_at);
        if exp > now {
            candidates.push(exp);
        }
    }
    if let Ok(t) = next_schedule_flip_after_default(cfg, now, hol) {
        if t > now {
            candidates.push(t);
        }
    }
    candidates
        .into_iter()
        .min()
        .unwrap_or_else(|| now + chrono::Duration::hours(1))
}

async fn update_status_cache(
    status: &Arc<RwLock<StatusCache>>,
    status_tx: &watch::Sender<String>,
    json: String,
    work_mode: bool,
    source: String,
    next_wake: String,
) {
    let prev = status.read().await.json.clone();
    {
        let mut cache = status.write().await;
        cache.json = json.clone();
        cache.work_mode = work_mode;
        cache.source = source;
        cache.next_wake = next_wake;
    }
    if json != prev {
        let _ = status_tx.send(json);
    }
}

async fn run_one_tick(
    cfg_path: &std::path::Path,
    state_path: &std::path::Path,
    status: &Arc<RwLock<StatusCache>>,
    config_toml: &Arc<RwLock<String>>,
    poke_gate: &PokeGate,
    status_tx: &watch::Sender<String>,
    force: bool,
    applied: &mut BackendApplied,
    retry_backoff: &mut Duration,
) -> Duration {
    let mut errors: Vec<String> = Vec::new();

    let tick: anyhow::Result<Duration> = async {
        let app = config::load_config(cfg_path).context("load config")?;
        let token = secret::get_token().await?.unwrap_or_default();
        if token.trim().is_empty() {
            errors.push("discord token missing (keyring or fallback file)".into());
        }
        let work = build_config(&app, &token).map_err(|e| anyhow::anyhow!(e))?;
        let raw = toml::to_string_pretty(&app).context("serialize config")?;
        *config_toml.write().await = raw;

        let mut state = config::load_state(state_path).context("load state")?;
        let now = Local::now();
        let year = now.year();
        let hol = load_holiday_nearby_years(&app.schedule.country, &app.schedule.subdiv, year)
            .map_err(|e| anyhow::anyhow!(e))?;

        let (mut work_mode, mut source, ov) =
            override_state::effective_work_mode(&work, now, &hol, &state);
        if app.mode.kind == "network" && ov.is_none() {
            work_mode = crate::network_probe::is_work_host_reachable(&app.mode).await;
            source = "network";
        }

        let immediate = poke_gate.take_immediate();
        let target_online = !work_mode;
        let should_apply = needs_apply(force, immediate, target_online, applied);

        let apply_result = if should_apply {
            apply_presence(&work, work_mode).await
        } else {
            ApplyResult {
                discord_ok: true,
                steam_ok: true,
                discord_error: None,
                steam_error: None,
            }
        };
        let apply_ok = apply_result.all_ok();
        let partial_failure = should_apply && !apply_ok;

        if should_apply {
            state.last_apply = Some(LastApplyRecord {
                ok: apply_ok,
                work_mode,
                source: source.to_string(),
                at: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
                discord_error: apply_result.discord_error.clone(),
                steam_error: apply_result.steam_error.clone(),
            });
            config::save_state(state_path, &state).context("save state")?;
            record_apply(applied, target_online, &apply_result);
        }

        let next = next_wake_time(now, &work, &hol, &state);
        let next_wake = Some(next.format("%Y-%m-%dT%H:%M:%S").to_string());

        let body = StatusSnapshot {
            ok: errors.is_empty() && (apply_ok || !should_apply),
            schema: STATUS_SCHEMA_VERSION,
            work_mode,
            source: source.to_string(),
            mode_kind: app.mode.kind.clone(),
            next_wake,
            last_apply: state.last_apply.clone(),
            errors: errors.clone(),
        };
        let json = serde_json::to_string(&body).unwrap_or_else(|_| "{\"ok\":false}".into());
        update_status_cache(
            status,
            status_tx,
            json,
            work_mode,
            source.to_string(),
            body.next_wake.clone().unwrap_or_default(),
        )
        .await;

        let dur = (next - Local::now())
            .to_std()
            .unwrap_or(Duration::from_secs(60));
        let schedule_sleep = dur.clamp(Duration::from_secs(1), Duration::from_secs(86400 * 7));
        let sleep = next_sleep_after_tick(schedule_sleep, partial_failure, retry_backoff);
        if partial_failure && sleep < schedule_sleep {
            tracing::warn!(
                target: "sdss::apply",
                retry_secs = sleep.as_secs(),
                "partial apply failure, scheduling retry"
            );
        }
        Ok(sleep)
    }
    .await;

    match tick {
        Ok(dur) => dur,
        Err(e) => {
            tracing::error!(target: "sdssd::scheduler", "tick error: {e:#}");
            let body = StatusSnapshot {
                ok: false,
                schema: STATUS_SCHEMA_VERSION,
                work_mode: false,
                source: "schedule".into(),
                mode_kind: "schedule".into(),
                next_wake: None,
                last_apply: None,
                errors: vec![format!("{e:#}")],
            };
            if let Ok(j) = serde_json::to_string(&body) {
                update_status_cache(
                    status,
                    status_tx,
                    j,
                    false,
                    "schedule".into(),
                    String::new(),
                )
                .await;
            }
            Duration::from_secs(60)
        }
    }
}

/// Drive apply + status updates until [`Notify`] fires.
pub async fn run(
    status: Arc<RwLock<StatusCache>>,
    config_toml: Arc<RwLock<String>>,
    poke_gate: Arc<PokeGate>,
    mut poke_rx: mpsc::Receiver<()>,
    shutdown: Arc<Notify>,
    status_tx: watch::Sender<String>,
) -> anyhow::Result<()> {
    let cfg_path = paths::config_file();
    let state_path = paths::state_file();

    let mut applied = BackendApplied::default();
    let mut apply_retry_backoff = APPLY_RETRY_INITIAL;

    let mut next_sleep = run_one_tick(
        &cfg_path,
        &state_path,
        &status,
        &config_toml,
        poke_gate.as_ref(),
        &status_tx,
        true,
        &mut applied,
        &mut apply_retry_backoff,
    )
    .await;

    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                tracing::info!(target: "sdssd::scheduler", "scheduler stopped");
                return Ok(());
            }
            Some(()) = poke_rx.recv() => {
                next_sleep = run_one_tick(
                    &cfg_path,
                    &state_path,
                    &status,
                    &config_toml,
                    poke_gate.as_ref(),
                    &status_tx,
                    false,
                    &mut applied,
                    &mut apply_retry_backoff,
                ).await;
            }
            _ = tokio::time::sleep(next_sleep) => {
                next_sleep = run_one_tick(
                    &cfg_path,
                    &state_path,
                    &status,
                    &config_toml,
                    poke_gate.as_ref(),
                    &status_tx,
                    false,
                    &mut applied,
                    &mut apply_retry_backoff,
                ).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_apply_when_discord_behind() {
        let applied = BackendApplied {
            discord_online: None,
            steam_online: Some(false),
        };
        assert!(needs_apply(false, false, false, &applied));
    }

    #[test]
    fn needs_apply_false_when_both_match() {
        let applied = BackendApplied {
            discord_online: Some(false),
            steam_online: Some(false),
        };
        assert!(!needs_apply(false, false, false, &applied));
    }

    #[test]
    fn record_apply_partial_leaves_failed_backend_stale() {
        let mut applied = BackendApplied {
            discord_online: Some(true),
            steam_online: Some(true),
        };
        record_apply(
            &mut applied,
            false,
            &ApplyResult {
                discord_ok: false,
                steam_ok: true,
                discord_error: Some("transport".into()),
                steam_error: None,
            },
        );
        assert_eq!(applied.discord_online, Some(true));
        assert_eq!(applied.steam_online, Some(false));
        assert!(needs_apply(false, false, false, &applied));
    }

    #[test]
    fn retry_sleep_shortens_after_partial_failure() {
        let mut backoff = APPLY_RETRY_INITIAL;
        let sleep = next_sleep_after_tick(Duration::from_secs(3600), true, &mut backoff);
        assert_eq!(sleep, APPLY_RETRY_INITIAL);
        assert_eq!(backoff, Duration::from_secs(60));
    }
}
