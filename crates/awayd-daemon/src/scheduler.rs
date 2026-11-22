//! SPDX-License-Identifier: GPL-3.0-or-later
//! Main scheduling loop: next wake deadline, poke channel, coordinated shutdown.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use awayd_core::backends::{self as backend_table, BackendInfo};
use awayd_core::config::schema::AppConfig;
use awayd_core::status::{
    BackendApplyRecord, BackendApplyStatus, LastApplyRecord, StatusSnapshot, SOURCE_ERROR,
    STATUS_SCHEMA_VERSION,
};
use chrono::{DateTime, Datelike, Local};
use tokio::sync::mpsc;
use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;

use crate::apply::{
    apply_presence, build_backend, ApplyPlan, ApplyResult, Backend, BackendOutcome,
};
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

/// State of one backend between ticks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BackendState {
    /// Last successfully applied `online` flag (`online` = personal / not work).
    online: Option<bool>,
    /// Credential the backend rejected permanently (fingerprint + error). The backend is not
    /// retried until the credential changes or the daemon restarts.
    rejected: Option<(u64, String)>,
    /// The service asked not to be contacted before this time (rate limit) + error.
    waiting: Option<(Instant, String)>,
}

/// Per backend id; backends without entry have not been applied yet.
type BackendApplied = HashMap<&'static str, BackendState>;

const APPLY_RETRY_INITIAL: Duration = Duration::from_secs(30);
const APPLY_RETRY_MAX: Duration = Duration::from_secs(120);

/// Longest sleep between two ticks. The wake time is computed from the wall clock, but the
/// sleep runs on a monotonic clock: after a change of system time or time zone, or a wake-up
/// Windows did not announce, the next tick recomputes it within this time. A tick without
/// change applies nothing.
const MAX_TICK_SLEEP: Duration = Duration::from_secs(15 * 60);

fn bounded_sleep(until_next_change: Duration) -> Duration {
    until_next_change.clamp(Duration::from_secs(1), MAX_TICK_SLEEP)
}

/// Network mode checks `work_host` at least every `probe_interval_seconds`: its presence can
/// change at any time, schedule changes do not matter there.
fn mode_sleep(sleep: Duration, mode: &awayd_core::config::schema::ModeSection) -> Duration {
    if mode.kind == "network" {
        sleep.min(Duration::from_secs(mode.probe_interval_seconds.max(1)))
    } else {
        sleep
    }
}

/// Services switched on in `app`, in table order.
fn enabled_backends(app: &AppConfig) -> impl Iterator<Item = &'static BackendInfo> + '_ {
    backend_table::ALL
        .iter()
        .filter(move |b| app.backend_enabled(b.id))
}

/// Backends of this tick (switched-on services only, so switched-off ones are never
/// contacted and their credential is not read). A backend whose credential is missing is
/// still listed (it skips itself); the missing credential is reported in `errors`.
async fn load_backends(
    app: &AppConfig,
    errors: &mut Vec<String>,
) -> anyhow::Result<Vec<Box<dyn Backend>>> {
    let mut list = Vec::new();
    for info in enabled_backends(app) {
        let credential = match info.credential_label {
            Some(label) => {
                let c = secret::get_credential(info.id).await?.unwrap_or_default();
                if c.trim().is_empty() {
                    errors.push(format!(
                        "{label} missing (Credential Manager or fallback file)"
                    ));
                }
                c
            }
            None => String::new(),
        };
        list.push(build_backend(info, credential));
    }
    Ok(list)
}

/// A backend rejected its current credential permanently.
fn is_blocked(applied: &BackendApplied, b: &dyn Backend) -> bool {
    matches!(
        applied.get(b.id()).and_then(|s| s.rejected.as_ref()),
        Some((fp, _)) if *fp == b.credential_fingerprint()
    )
}

/// The backend asked not to be contacted before a time that has not come yet (rate limit).
fn is_waiting(applied: &BackendApplied, id: &str, now: Instant) -> bool {
    applied
        .get(id)
        .and_then(|s| s.waiting.as_ref())
        .is_some_and(|(until, _)| *until > now)
}

/// Time until the first waiting backend may be contacted again.
fn earliest_wait(
    applied: &BackendApplied,
    backends: &[Box<dyn Backend>],
    now: Instant,
) -> Option<Duration> {
    backends
        .iter()
        .filter_map(|b| applied.get(b.id())?.waiting.as_ref())
        .filter(|(until, _)| *until > now)
        .map(|(until, _)| until.duration_since(now))
        .min()
}

/// Forget backends that are no longer in the list: one that comes back starts fresh.
fn forget_removed(applied: &mut BackendApplied, backends: &[Box<dyn Backend>]) {
    applied.retain(|id, _| backends.iter().any(|b| b.id() == *id));
}

/// Per backend: apply when forced (startup), poked (`immediate`) or not yet in the target
/// state. Only stale backends are applied, so a retry of one backend does not re-trigger the
/// others. A permanently rejected credential, and a running rate limit, hold the backend back
/// even for pokes: the poke source (user, resume, process watcher) is not known here, and a
/// request during a rate limit would only be rejected again.
fn plan_apply(
    force: bool,
    immediate: bool,
    target_online: bool,
    applied: &BackendApplied,
    backends: &[Box<dyn Backend>],
    now: Instant,
) -> ApplyPlan {
    let ids = backends
        .iter()
        .filter(|b| {
            let online = applied.get(b.id()).and_then(|s| s.online);
            !is_blocked(applied, b.as_ref())
                && !is_waiting(applied, b.id(), now)
                && (force || immediate || online != Some(target_online))
        })
        .map(|b| b.id())
        .collect();
    ApplyPlan { ids }
}

fn record_apply(
    applied: &mut BackendApplied,
    target_online: bool,
    result: &ApplyResult,
    backends: &[Box<dyn Backend>],
    now: Instant,
) {
    for (id, outcome) in &result.outcomes {
        let fp = backends
            .iter()
            .find(|b| b.id() == *id)
            .map(|b| b.credential_fingerprint())
            .unwrap_or_default();
        let st = applied.entry(*id).or_default();
        match outcome {
            BackendOutcome::Sent => {
                st.online = Some(target_online);
                st.rejected = None;
                st.waiting = None;
            }
            BackendOutcome::Failed {
                error,
                permanent: true,
                ..
            } => st.rejected = Some((fp, error.clone())),
            BackendOutcome::Failed {
                error,
                retry_after: Some(delay),
                ..
            } => st.waiting = Some((now + *delay, error.clone())),
            // Not applied (program not running, no credential): apply again once possible
            // (process watcher poke, credential change or next tick).
            BackendOutcome::Skipped(_) => st.online = None,
            _ => {}
        }
    }
}

/// Status messages for backends blocked on their current credential.
fn blocked_messages(applied: &BackendApplied, backends: &[Box<dyn Backend>]) -> Vec<String> {
    backends
        .iter()
        .filter(|b| is_blocked(applied, b.as_ref()))
        .filter_map(|b| {
            let (_, error) = applied.get(b.id())?.rejected.as_ref()?;
            let what = backend_table::find(b.id())
                .and_then(|i| i.credential_label)
                .unwrap_or("credential");
            Some(format!(
                "{}: {error}; not retried automatically until the {what} is changed",
                b.id()
            ))
        })
        .collect()
}

/// Status messages for backends held back by a rate limit, with the local time of the next
/// attempt.
fn waiting_messages(
    applied: &BackendApplied,
    backends: &[Box<dyn Backend>],
    now: Instant,
    now_local: DateTime<Local>,
) -> Vec<String> {
    backends
        .iter()
        .filter_map(|b| {
            let (until, error) = applied.get(b.id())?.waiting.as_ref()?;
            let left = until.checked_duration_since(now)?;
            let at = now_local + chrono::Duration::from_std(left).ok()?;
            Some(format!(
                "{}: {error}; next attempt at {}",
                b.id(),
                at.format("%H:%M:%S")
            ))
        })
        .collect()
}

/// Status entries for the backends applied in this run.
fn apply_records(result: &ApplyResult) -> Vec<BackendApplyRecord> {
    result
        .outcomes
        .iter()
        .filter_map(|(id, outcome)| {
            let (status, message) = match outcome {
                BackendOutcome::NotAttempted => return None,
                BackendOutcome::Sent => (BackendApplyStatus::Sent, None),
                BackendOutcome::Skipped(reason) => {
                    (BackendApplyStatus::Skipped, Some(reason.clone()))
                }
                BackendOutcome::Failed { error, .. } => {
                    (BackendApplyStatus::Failed, Some(error.clone()))
                }
            };
            Some(BackendApplyRecord {
                id: (*id).to_string(),
                status,
                message,
            })
        })
        .collect()
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
        let backends = load_backends(&app, &mut errors).await?;
        let work = build_config(&app).map_err(|e| anyhow::anyhow!(e))?;
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
        forget_removed(applied, &backends);
        let plan = plan_apply(
            force,
            immediate,
            target_online,
            applied,
            &backends,
            Instant::now(),
        );
        let should_apply = plan.any();

        let apply_result = if should_apply {
            apply_presence(&backends, work_mode, &plan).await
        } else {
            ApplyResult::not_attempted()
        };
        let apply_ok = apply_result.all_ok();
        // Retry only errors a retry may fix; skipped backends and rejected tokens are not
        // retried, rate-limited ones wait for the time the service gave (`earliest_wait`).
        let partial_failure = should_apply && apply_result.needs_retry();

        if should_apply {
            let record = LastApplyRecord {
                ok: apply_ok,
                work_mode,
                source: source.to_string(),
                at: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
                backends: apply_records(&apply_result),
            };
            // Re-load and change only `last_apply`: an override set while the apply was
            // running must not be overwritten with the copy loaded at the start of the tick.
            state = update_state(state_path, |st| st.last_apply = Some(record))
                .context("save state")?;
            record_apply(
                applied,
                target_online,
                &apply_result,
                &backends,
                Instant::now(),
            );
        }
        errors.extend(blocked_messages(applied, &backends));
        errors.extend(waiting_messages(
            applied,
            &backends,
            Instant::now(),
            Local::now(),
        ));

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
            version: awayd_core::VERSION.into(),
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
        let schedule_sleep = bounded_sleep(dur);
        let mut sleep = next_sleep_after_tick(schedule_sleep, partial_failure, retry_backoff);
        if partial_failure && sleep < schedule_sleep {
            tracing::warn!(
                target: "awayd::apply",
                retry_secs = sleep.as_secs(),
                "partial apply failure, scheduling retry"
            );
        }
        // Wake when a rate-limited backend may be contacted again.
        if let Some(wait) = earliest_wait(applied, &backends, Instant::now()) {
            sleep = sleep.min(wait.max(Duration::from_secs(1)));
        }
        Ok(mode_sleep(sleep, &app.mode))
    }
    .await;

    match tick {
        Ok(dur) => dur,
        Err(e) => {
            tracing::error!(target: "awayd::scheduler", "tick error: {e:#}");
            // `source = "error"`: the tray must not show the normal "online" state when
            // nothing was applied.
            let body = StatusSnapshot {
                ok: false,
                schema: STATUS_SCHEMA_VERSION,
                work_mode: false,
                source: SOURCE_ERROR.into(),
                mode_kind: "schedule".into(),
                next_wake: None,
                last_apply: None,
                errors: vec![format!("{e:#}")],
                version: awayd_core::VERSION.into(),
            };
            if let Ok(j) = serde_json::to_string(&body) {
                update_status_cache(
                    status,
                    status_tx,
                    j,
                    false,
                    SOURCE_ERROR.into(),
                    String::new(),
                )
                .await;
            }
            Duration::from_secs(60)
        }
    }
}

/// Drive apply + status updates until `shutdown` is cancelled (a cancel during a tick is not
/// lost: the token keeps its state and is seen once the tick returns).
pub async fn run(
    status: Arc<RwLock<StatusCache>>,
    config_toml: Arc<RwLock<String>>,
    poke_gate: Arc<PokeGate>,
    mut poke_rx: mpsc::Receiver<()>,
    shutdown: CancellationToken,
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
            _ = shutdown.cancelled() => {
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
    use async_trait::async_trait;
    use chrono::TimeZone;

    /// Any backend id works: the scheduler has no per-service code.
    struct Dummy {
        id: &'static str,
        fp: u64,
    }

    #[async_trait]
    impl Backend for Dummy {
        fn id(&self) -> &'static str {
            self.id
        }
        async fn apply(&self, _online: bool) -> BackendOutcome {
            BackendOutcome::Sent
        }
        fn credential_fingerprint(&self) -> u64 {
            self.fp
        }
    }

    fn list(ids: &[(&'static str, u64)]) -> Vec<Box<dyn Backend>> {
        ids.iter()
            .map(|&(id, fp)| Box::new(Dummy { id, fp }) as Box<dyn Backend>)
            .collect()
    }

    fn applied(entries: &[(&'static str, Option<bool>)]) -> BackendApplied {
        entries
            .iter()
            .map(|&(id, online)| {
                (
                    id,
                    BackendState {
                        online,
                        rejected: None,
                        waiting: None,
                    },
                )
            })
            .collect()
    }

    fn result(outcomes: Vec<(&'static str, BackendOutcome)>) -> ApplyResult {
        ApplyResult { outcomes }
    }

    fn failed(permanent: bool) -> BackendOutcome {
        BackendOutcome::Failed {
            error: "err".into(),
            permanent,
            retry_after: None,
        }
    }

    const CRED: u64 = 1;
    const OTHER_CRED: u64 = 2;

    /// Fixed "now" so planning and recording compare against the same instant.
    fn t0() -> Instant {
        static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *T0.get_or_init(Instant::now)
    }

    fn rate_limited(secs: u64) -> BackendOutcome {
        BackendOutcome::Failed {
            error: format!("rate limited (retry after {secs}s)"),
            permanent: false,
            retry_after: Some(Duration::from_secs(secs)),
        }
    }

    #[test]
    fn plan_applies_only_the_stale_backends() {
        let b = list(&[("chat", CRED), ("games", 0), ("third", 0)]);
        let a = applied(&[("games", Some(false))]);
        let plan = plan_apply(false, false, false, &a, &b, t0());
        assert_eq!(
            plan.ids,
            vec!["chat", "third"],
            "a retry of one backend must not re-trigger the others"
        );
    }

    #[test]
    fn plan_is_empty_when_all_match() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let a = applied(&[("chat", Some(false)), ("games", Some(false))]);
        assert!(!plan_apply(false, false, false, &a, &b, t0()).any());
    }

    #[test]
    fn poke_and_force_apply_all() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let a = applied(&[("chat", Some(false)), ("games", Some(false))]);
        assert_eq!(plan_apply(false, true, false, &a, &b, t0()).ids.len(), 2);
        assert_eq!(plan_apply(true, false, false, &a, &b, t0()).ids.len(), 2);
    }

    #[test]
    fn rejected_credential_blocks_backend_until_it_changes() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let mut a = applied(&[("games", Some(false))]);
        record_apply(
            &mut a,
            false,
            &result(vec![("chat", failed(true))]),
            &b,
            t0(),
        );
        for (force, immediate) in [(false, false), (false, true), (true, true)] {
            assert!(
                !plan_apply(force, immediate, false, &a, &b, t0()).contains("chat"),
                "blocked for force={force} immediate={immediate}"
            );
        }
        assert_eq!(blocked_messages(&a, &b).len(), 1);

        let changed = list(&[("chat", OTHER_CRED), ("games", 0)]);
        assert!(plan_apply(false, false, false, &a, &changed, t0()).contains("chat"));
        assert!(blocked_messages(&a, &changed).is_empty());

        record_apply(
            &mut a,
            false,
            &result(vec![("chat", BackendOutcome::Sent)]),
            &changed,
            t0(),
        );
        assert_eq!(
            a["chat"],
            BackendState {
                online: Some(false),
                rejected: None,
                waiting: None
            }
        );
    }

    #[test]
    fn blocked_message_names_the_credential() {
        let b = list(&[(backend_table::DISCORD, CRED)]);
        let mut a = BackendApplied::new();
        record_apply(
            &mut a,
            false,
            &result(vec![(backend_table::DISCORD, failed(true))]),
            &b,
            t0(),
        );
        assert_eq!(
            blocked_messages(&a, &b),
            vec!["discord: err; not retried automatically until the Discord token is changed"]
        );
    }

    #[test]
    fn transient_failure_leaves_backend_stale_and_retries() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let mut a = applied(&[("chat", Some(true)), ("games", Some(true))]);
        let r = result(vec![
            ("chat", failed(false)),
            ("games", BackendOutcome::Sent),
        ]);
        record_apply(&mut a, false, &r, &b, t0());
        assert_eq!(a["chat"].online, Some(true));
        assert_eq!(a["games"].online, Some(false));
        assert!(a["chat"].rejected.is_none());
        assert!(r.needs_retry());
        assert_eq!(
            plan_apply(false, false, false, &a, &b, t0()).ids,
            vec!["chat"]
        );
    }

    #[test]
    fn skipped_and_permanent_outcomes_do_not_retry() {
        let skipped = result(vec![
            ("chat", BackendOutcome::Skipped("no token".into())),
            ("games", BackendOutcome::Skipped("not running".into())),
        ]);
        assert!(!skipped.needs_retry());
        assert!(skipped.all_ok());

        let rejected = result(vec![
            ("chat", failed(true)),
            ("games", BackendOutcome::Sent),
        ]);
        assert!(!rejected.needs_retry());
        assert!(!rejected.all_ok());
    }

    #[test]
    fn skipped_backend_is_applied_again_later() {
        let b = list(&[("games", 0)]);
        let mut a = applied(&[("games", Some(false))]);
        record_apply(
            &mut a,
            false,
            &result(vec![(
                "games",
                BackendOutcome::Skipped("not running".into()),
            )]),
            &b,
            t0(),
        );
        assert_eq!(a["games"].online, None);
        assert!(plan_apply(false, false, false, &a, &b, t0()).contains("games"));
    }

    #[test]
    fn switched_off_services_are_not_loaded() {
        let mut app = AppConfig::default();
        app.set_backend_enabled(backend_table::DISCORD, false);
        app.set_backend_enabled(backend_table::STEAM, true);
        let ids: Vec<_> = enabled_backends(&app).map(|b| b.id).collect();
        assert_eq!(ids, vec![backend_table::STEAM]);

        app.set_backend_enabled(backend_table::STEAM, false);
        assert_eq!(enabled_backends(&app).count(), 0);

        app.backends.clear();
        assert_eq!(enabled_backends(&app).count(), backend_table::ALL.len());
    }

    #[test]
    fn removed_backend_is_forgotten() {
        let mut a = applied(&[("chat", Some(false)), ("games", Some(false))]);
        forget_removed(&mut a, &list(&[("games", 0)]));
        assert!(!a.contains_key("chat"));
        assert!(a.contains_key("games"));
    }

    #[test]
    fn apply_records_list_attempted_backends() {
        let r = result(vec![
            ("chat", BackendOutcome::Sent),
            ("games", BackendOutcome::Skipped("not running".into())),
            ("third", failed(false)),
            ("fourth", BackendOutcome::NotAttempted),
        ]);
        let recs = apply_records(&r);
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].status, BackendApplyStatus::Sent);
        assert_eq!(recs[1].message.as_deref(), Some("not running"));
        assert_eq!(recs[2].status, BackendApplyStatus::Failed);
    }

    #[test]
    fn retry_sleep_shortens_after_partial_failure() {
        let mut backoff = APPLY_RETRY_INITIAL;
        let sleep = next_sleep_after_tick(Duration::from_secs(3600), true, &mut backoff);
        assert_eq!(sleep, APPLY_RETRY_INITIAL);
        assert_eq!(backoff, Duration::from_secs(60));
    }

    #[test]
    fn rate_limited_backend_waits_even_for_pokes_others_continue() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let mut a = applied(&[("games", Some(false))]);
        let r = result(vec![("chat", rate_limited(600))]);
        // Not a case for the scheduler's own 30/60/120 s backoff.
        assert!(!r.needs_retry());
        record_apply(&mut a, false, &r, &b, t0());

        let later = t0() + Duration::from_secs(60);
        for (force, immediate) in [(false, false), (false, true), (true, true)] {
            let plan = plan_apply(force, immediate, false, &a, &b, later);
            assert!(
                !plan.contains("chat"),
                "force={force} immediate={immediate}"
            );
        }
        // The other backend is still applied on a poke.
        assert!(plan_apply(false, true, false, &a, &b, later).contains("games"));

        // Wake exactly when the wait is over, then the backend is planned again.
        assert_eq!(earliest_wait(&a, &b, later), Some(Duration::from_secs(540)));
        let after = t0() + Duration::from_secs(600);
        assert_eq!(earliest_wait(&a, &b, after), None);
        assert!(plan_apply(false, false, false, &a, &b, after).contains("chat"));
    }

    #[test]
    fn waiting_message_names_next_attempt_and_clears_after_success() {
        let b = list(&[("chat", CRED)]);
        let mut a = BackendApplied::new();
        record_apply(
            &mut a,
            false,
            &result(vec![("chat", rate_limited(90))]),
            &b,
            t0(),
        );
        let local = chrono::Local
            .with_ymd_and_hms(2026, 11, 22, 19, 40, 40)
            .unwrap();
        assert_eq!(
            waiting_messages(&a, &b, t0(), local),
            vec!["chat: rate limited (retry after 90s); next attempt at 19:42:10"]
        );

        let after = t0() + Duration::from_secs(90);
        record_apply(
            &mut a,
            false,
            &result(vec![("chat", BackendOutcome::Sent)]),
            &b,
            after,
        );
        assert!(waiting_messages(&a, &b, after, local).is_empty());
        assert!(a["chat"].waiting.is_none());
    }

    /// The reason for `CancellationToken` (legacy issue 12): a shutdown signalled while the
    /// loop is busy in a tick (nobody waiting) must still be seen at the next wait;
    /// `Notify::notify_waiters` lost it.
    #[tokio::test]
    async fn shutdown_signalled_before_waiting_is_not_lost() {
        let token = CancellationToken::new();
        token.cancel();
        let seen = tokio::time::timeout(Duration::from_secs(1), token.cancelled()).await;
        assert!(seen.is_ok());
    }

    #[test]
    fn sleep_is_bounded_so_clock_changes_are_picked_up() {
        assert_eq!(
            bounded_sleep(Duration::from_secs(86_400)),
            MAX_TICK_SLEEP,
            "a change tomorrow still wakes within 15 minutes"
        );
        assert_eq!(
            bounded_sleep(Duration::from_secs(120)),
            Duration::from_secs(120)
        );
        assert_eq!(bounded_sleep(Duration::ZERO), Duration::from_secs(1));
    }

    #[test]
    fn network_mode_wakes_at_the_probe_interval() {
        let mut mode = awayd_core::config::schema::ModeSection::default();
        let long = Duration::from_secs(900);
        assert_eq!(mode_sleep(long, &mode), long, "schedule mode unchanged");

        mode.kind = "network".into();
        mode.probe_interval_seconds = 60;
        assert_eq!(mode_sleep(long, &mode), Duration::from_secs(60));
        // A sooner wake (schedule, retry) is kept.
        assert_eq!(
            mode_sleep(Duration::from_secs(20), &mode),
            Duration::from_secs(20)
        );
    }
}
