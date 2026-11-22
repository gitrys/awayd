//! SPDX-License-Identifier: GPL-3.0-or-later
//! Main scheduling loop: next wake deadline, poke channel, coordinated shutdown.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use awayd_core::backends as backend_table;
use awayd_core::status::{
    BackendApplyRecord, BackendApplyStatus, LastApplyRecord, StatusSnapshot, SOURCE_ERROR,
    STATUS_SCHEMA_VERSION,
};
use chrono::{DateTime, Datelike, Local};
use tokio::sync::mpsc;
use tokio::sync::{watch, Notify, RwLock};

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
}

/// Per backend id; backends without entry have not been applied yet.
type BackendApplied = HashMap<&'static str, BackendState>;

const APPLY_RETRY_INITIAL: Duration = Duration::from_secs(30);
const APPLY_RETRY_MAX: Duration = Duration::from_secs(120);

/// Backends of this tick, with their stored credentials. A backend whose credential is
/// missing is still listed (it skips itself); the missing credential is reported in `errors`.
async fn load_backends(errors: &mut Vec<String>) -> anyhow::Result<Vec<Box<dyn Backend>>> {
    let mut list = Vec::new();
    for info in backend_table::ALL {
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

/// Forget backends that are no longer in the list: one that comes back starts fresh.
fn forget_removed(applied: &mut BackendApplied, backends: &[Box<dyn Backend>]) {
    applied.retain(|id, _| backends.iter().any(|b| b.id() == *id));
}

/// Per backend: apply when forced (startup), poked (`immediate`) or not yet in the target
/// state. Only stale backends are applied, so a retry of one backend does not re-trigger the
/// others. A permanently rejected credential blocks its backend even for pokes: the poke
/// source (user, resume, process watcher) is not known here.
fn plan_apply(
    force: bool,
    immediate: bool,
    target_online: bool,
    applied: &BackendApplied,
    backends: &[Box<dyn Backend>],
) -> ApplyPlan {
    let ids = backends
        .iter()
        .filter(|b| {
            let online = applied.get(b.id()).and_then(|s| s.online);
            !is_blocked(applied, b.as_ref())
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
            }
            BackendOutcome::Failed {
                error,
                permanent: true,
            } => st.rejected = Some((fp, error.clone())),
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
        let backends = load_backends(&mut errors).await?;
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
        let plan = plan_apply(force, immediate, target_online, applied, &backends);
        let should_apply = plan.any();

        let apply_result = if should_apply {
            apply_presence(&backends, work_mode, &plan).await
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
                backends: apply_records(&apply_result),
            };
            // Re-load and change only `last_apply`: an override set while the apply was
            // running must not be overwritten with the copy loaded at the start of the tick.
            state = update_state(state_path, |st| st.last_apply = Some(record))
                .context("save state")?;
            record_apply(applied, target_online, &apply_result, &backends);
        }
        errors.extend(blocked_messages(applied, &backends));

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
    use async_trait::async_trait;

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
        }
    }

    const CRED: u64 = 1;
    const OTHER_CRED: u64 = 2;

    #[test]
    fn plan_applies_only_the_stale_backends() {
        let b = list(&[("chat", CRED), ("games", 0), ("third", 0)]);
        let a = applied(&[("games", Some(false))]);
        let plan = plan_apply(false, false, false, &a, &b);
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
        assert!(!plan_apply(false, false, false, &a, &b).any());
    }

    #[test]
    fn poke_and_force_apply_all() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let a = applied(&[("chat", Some(false)), ("games", Some(false))]);
        assert_eq!(plan_apply(false, true, false, &a, &b).ids.len(), 2);
        assert_eq!(plan_apply(true, false, false, &a, &b).ids.len(), 2);
    }

    #[test]
    fn rejected_credential_blocks_backend_until_it_changes() {
        let b = list(&[("chat", CRED), ("games", 0)]);
        let mut a = applied(&[("games", Some(false))]);
        record_apply(&mut a, false, &result(vec![("chat", failed(true))]), &b);
        for (force, immediate) in [(false, false), (false, true), (true, true)] {
            assert!(
                !plan_apply(force, immediate, false, &a, &b).contains("chat"),
                "blocked for force={force} immediate={immediate}"
            );
        }
        assert_eq!(blocked_messages(&a, &b).len(), 1);

        let changed = list(&[("chat", OTHER_CRED), ("games", 0)]);
        assert!(plan_apply(false, false, false, &a, &changed).contains("chat"));
        assert!(blocked_messages(&a, &changed).is_empty());

        record_apply(
            &mut a,
            false,
            &result(vec![("chat", BackendOutcome::Sent)]),
            &changed,
        );
        assert_eq!(
            a["chat"],
            BackendState {
                online: Some(false),
                rejected: None
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
        record_apply(&mut a, false, &r, &b);
        assert_eq!(a["chat"].online, Some(true));
        assert_eq!(a["games"].online, Some(false));
        assert!(a["chat"].rejected.is_none());
        assert!(r.needs_retry());
        assert_eq!(plan_apply(false, false, false, &a, &b).ids, vec!["chat"]);
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
        );
        assert_eq!(a["games"].online, None);
        assert!(plan_apply(false, false, false, &a, &b).contains("games"));
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
}
