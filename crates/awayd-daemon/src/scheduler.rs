//! SPDX-License-Identifier: GPL-3.0-or-later
//! Main scheduling loop: next wake deadline, poke channel, coordinated shutdown.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use awayd_core::status::{LastApplyRecord, StatusSnapshot, STATUS_SCHEMA_VERSION};
use chrono::{DateTime, Datelike, Local};
use tokio::sync::mpsc;
use tokio::sync::{watch, Notify, RwLock};

use crate::apply::{apply_presence, ApplyPlan, ApplyResult, BackendOutcome};
use crate::holidays::{load_holidays_or_empty, HolidaySet};
use crate::local_time::{resolve_local, trim_to_minute};
use crate::override_state;
use crate::runtime_config::build_config;
use crate::schedule_next::next_schedule_flip_after_default;
use crate::secret;
use crate::service::StatusCache;
use crate::state_store::update_state;
use crate::watchers::PokeGate;
use awayd_core::config::paths;
use awayd_core::config::state::AppState;
use awayd_core::config::{self};

/// Last successfully applied `online` flag per backend (`online` = personal / not work).
#[derive(Debug, Clone, Default)]
struct BackendApplied {
    discord_online: Option<bool>,
    steam_online: Option<bool>,
    /// Token Discord rejected permanently (fingerprint + error). Discord is not retried
    /// until the token changes or the daemon restarts.
    discord_rejected: Option<(u64, String)>,
}

const APPLY_RETRY_INITIAL: Duration = Duration::from_secs(30);
const APPLY_RETRY_MAX: Duration = Duration::from_secs(120);

/// In-memory fingerprint to notice a token change without keeping another copy of it.
fn token_fingerprint(token: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    token.trim().hash(&mut h);
    h.finish()
}

/// Per backend: apply when forced (startup), poked (`immediate`) or not yet in the target
/// state. Only the stale backend is applied, so a Discord retry no longer re-triggers Steam.
/// A permanently rejected token blocks Discord even for pokes: the poke source (user, resume,
/// process watcher) is not known here.
fn plan_apply(
    force: bool,
    immediate: bool,
    target_online: bool,
    applied: &BackendApplied,
    token_fp: u64,
) -> ApplyPlan {
    let discord_blocked = matches!(&applied.discord_rejected, Some((fp, _)) if *fp == token_fp);
    ApplyPlan {
        discord: !discord_blocked
            && (force || immediate || applied.discord_online != Some(target_online)),
        steam: force || immediate || applied.steam_online != Some(target_online),
    }
}

fn record_apply(
    applied: &mut BackendApplied,
    target_online: bool,
    result: &ApplyResult,
    token_fp: u64,
) {
    match &result.discord {
        BackendOutcome::Sent => {
            applied.discord_online = Some(target_online);
            applied.discord_rejected = None;
        }
        BackendOutcome::Failed {
            error,
            permanent: true,
        } => applied.discord_rejected = Some((token_fp, error.clone())),
        _ => {}
    }
    match &result.steam {
        BackendOutcome::Sent => applied.steam_online = Some(target_online),
        // Not running: apply again once it runs (process watcher poke or next tick).
        BackendOutcome::Skipped(_) => applied.steam_online = None,
        _ => {}
    }
}

/// Status message while Discord is blocked for the current token.
fn discord_blocked_message(applied: &BackendApplied, token_fp: u64) -> Option<String> {
    match &applied.discord_rejected {
        Some((fp, error)) if *fp == token_fp => Some(format!(
            "discord: {error}; not retried automatically until the token is changed"
        )),
        _ => None,
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
        let (hol, hol_warning) =
            load_holidays_or_empty(&app.schedule.country, &app.schedule.subdiv, year);
        if let Some(w) = hol_warning {
            tracing::warn!(target: "awayd::holidays", "{w}");
            errors.push(w);
        }

        let (mut work_mode, mut source, ov) =
            override_state::effective_work_mode(&work, now, &hol, &state);
        if app.mode.kind == "network" && ov.is_none() {
            work_mode = crate::network_probe::is_work_host_reachable(&app.mode).await;
            source = "network";
        }

        let immediate = poke_gate.take_immediate();
        let target_online = !work_mode;
        let token_fp = token_fingerprint(&token);
        let plan = plan_apply(force, immediate, target_online, applied, token_fp);
        let should_apply = plan.any();

        let apply_result = if should_apply {
            apply_presence(&work, work_mode, plan).await
        } else {
            ApplyResult::not_attempted()
        };
        let apply_ok = apply_result.all_ok();
        // Retry only errors a retry may fix; skipped backends and rejected tokens are not
        // retried.
        let partial_failure = should_apply && apply_result.needs_retry();

        if should_apply {
            let record = LastApplyRecord {
                ok: apply_ok,
                work_mode,
                source: source.to_string(),
                at: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
                discord_error: apply_result.discord.error(),
                steam_error: apply_result.steam.error(),
            };
            // Re-load and change only `last_apply`: an override set while the apply was
            // running must not be overwritten with the copy loaded at the start of the tick.
            state = update_state(state_path, |st| st.last_apply = Some(record))
                .context("save state")?;
            record_apply(applied, target_online, &apply_result, token_fp);
        }
        if let Some(msg) = discord_blocked_message(applied, token_fp) {
            errors.push(msg);
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
                target: "awayd::apply",
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
            tracing::error!(target: "awayd::scheduler", "tick error: {e:#}");
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
                tracing::info!(target: "awayd::scheduler", "scheduler stopped");
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

    const TOKEN: u64 = 1;
    const OTHER_TOKEN: u64 = 2;

    fn applied(discord: Option<bool>, steam: Option<bool>) -> BackendApplied {
        BackendApplied {
            discord_online: discord,
            steam_online: steam,
            discord_rejected: None,
        }
    }

    fn result(discord: BackendOutcome, steam: BackendOutcome) -> ApplyResult {
        ApplyResult { discord, steam }
    }

    fn failed(permanent: bool) -> BackendOutcome {
        BackendOutcome::Failed {
            error: "err".into(),
            permanent,
        }
    }

    #[test]
    fn plan_applies_only_the_stale_backend() {
        let a = applied(None, Some(false));
        let plan = plan_apply(false, false, false, &a, TOKEN);
        assert_eq!(
            plan,
            ApplyPlan {
                discord: true,
                steam: false
            },
            "a Discord retry must not re-trigger Steam"
        );
    }

    #[test]
    fn plan_is_empty_when_both_match() {
        let a = applied(Some(false), Some(false));
        assert!(!plan_apply(false, false, false, &a, TOKEN).any());
    }

    #[test]
    fn poke_applies_both() {
        let a = applied(Some(false), Some(false));
        let plan = plan_apply(false, true, false, &a, TOKEN);
        assert!(plan.discord && plan.steam);
    }

    #[test]
    fn rejected_token_blocks_discord_until_token_changes() {
        let mut a = applied(None, Some(false));
        record_apply(
            &mut a,
            false,
            &result(failed(true), BackendOutcome::NotAttempted),
            TOKEN,
        );
        for (force, immediate) in [(false, false), (false, true), (true, true)] {
            assert!(
                !plan_apply(force, immediate, false, &a, TOKEN).discord,
                "blocked for force={force} immediate={immediate}"
            );
        }
        assert!(discord_blocked_message(&a, TOKEN).is_some());

        assert!(plan_apply(false, false, false, &a, OTHER_TOKEN).discord);
        assert!(discord_blocked_message(&a, OTHER_TOKEN).is_none());

        record_apply(
            &mut a,
            false,
            &result(BackendOutcome::Sent, BackendOutcome::NotAttempted),
            OTHER_TOKEN,
        );
        assert!(a.discord_rejected.is_none());
        assert_eq!(a.discord_online, Some(false));
    }

    #[test]
    fn transient_failure_leaves_backend_stale_and_retries() {
        let mut a = applied(Some(true), Some(true));
        let r = result(failed(false), BackendOutcome::Sent);
        record_apply(&mut a, false, &r, TOKEN);
        assert_eq!(a.discord_online, Some(true));
        assert_eq!(a.steam_online, Some(false));
        assert!(a.discord_rejected.is_none());
        assert!(r.needs_retry());
        assert!(plan_apply(false, false, false, &a, TOKEN).discord);
    }

    #[test]
    fn skipped_and_permanent_outcomes_do_not_retry() {
        let skipped = result(
            BackendOutcome::Skipped("no discord token configured".into()),
            BackendOutcome::Skipped("steam is not running".into()),
        );
        assert!(!skipped.needs_retry());
        assert!(skipped.all_ok());

        let rejected = result(failed(true), BackendOutcome::Sent);
        assert!(!rejected.needs_retry());
        assert!(!rejected.all_ok());
    }

    #[test]
    fn steam_not_running_is_applied_again_later() {
        let mut a = applied(Some(false), Some(false));
        record_apply(
            &mut a,
            false,
            &result(
                BackendOutcome::NotAttempted,
                BackendOutcome::Skipped("steam is not running".into()),
            ),
            TOKEN,
        );
        assert_eq!(a.steam_online, None);
        assert!(plan_apply(false, false, false, &a, TOKEN).steam);
    }

    #[test]
    fn token_fingerprint_ignores_surrounding_whitespace() {
        assert_eq!(token_fingerprint(" abc "), token_fingerprint("abc"));
        assert_ne!(token_fingerprint("abc"), token_fingerprint("abd"));
    }

    #[test]
    fn retry_sleep_shortens_after_partial_failure() {
        let mut backoff = APPLY_RETRY_INITIAL;
        let sleep = next_sleep_after_tick(Duration::from_secs(3600), true, &mut backoff);
        assert_eq!(sleep, APPLY_RETRY_INITIAL);
        assert_eq!(backoff, Duration::from_secs(60));
    }
}
