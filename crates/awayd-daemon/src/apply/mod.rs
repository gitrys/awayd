//! SPDX-License-Identifier: GPL-3.0-or-later
//! Apply presence to each backend independently (no short-circuit), only for the backends
//! the scheduler selected.

pub mod discord;
pub mod steam;

use std::time::Duration;

use async_trait::async_trait;
use awayd_core::backends::{self, BackendInfo};
use tracing::{info, warn};

/// One status service. Static data (name, processes, credential) is in
/// [`awayd_core::backends::ALL`]; this trait is the behaviour.
#[async_trait]
pub trait Backend: Send + Sync {
    fn id(&self) -> &'static str;

    /// Set the status: `online` = personal time, otherwise invisible.
    async fn apply(&self, online: bool) -> BackendOutcome;

    /// Fingerprint of the credential in use. After a permanent failure the backend is not
    /// retried until this value changes. Backends without credential keep the default.
    fn credential_fingerprint(&self) -> u64 {
        0
    }
}

/// Create the backend for `info`. `credential` is the stored value for backends with a
/// credential (empty when none is stored).
pub fn build_backend(info: &BackendInfo, credential: String) -> Box<dyn Backend> {
    match info.id {
        backends::DISCORD => Box::new(DiscordBackend { token: credential }),
        backends::STEAM => Box::new(SteamBackend),
        other => unreachable!("backend {other} in awayd_core::backends::ALL has no implementation"),
    }
}

/// Normalize a credential before it is stored (e.g. strip a pasted `Bearer ` prefix).
pub fn normalize_credential(id: &str, raw: &str) -> String {
    match id {
        backends::DISCORD => discord::normalize_token(raw),
        _ => raw.trim().to_string(),
    }
}

/// In-memory fingerprint to notice a credential change without keeping another copy of it.
pub fn fingerprint(credential: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    credential.trim().hash(&mut h);
    h.finish()
}

pub struct DiscordBackend {
    token: String,
}

#[async_trait]
impl Backend for DiscordBackend {
    fn id(&self) -> &'static str {
        backends::DISCORD
    }

    async fn apply(&self, online: bool) -> BackendOutcome {
        if self.token.trim().is_empty() {
            return BackendOutcome::Skipped("no discord token configured".into());
        }
        match discord::patch_status(&self.token, online).await {
            Ok(_) => BackendOutcome::Sent,
            Err(e) => BackendOutcome::Failed {
                error: e.to_string(),
                permanent: e.is_permanent(),
                retry_after: match e {
                    discord::DiscordError::RateLimited { retry_after_secs } => {
                        Some(Duration::from_secs(retry_after_secs))
                    }
                    _ => None,
                },
            },
        }
    }

    fn credential_fingerprint(&self) -> u64 {
        fingerprint(&self.token)
    }
}

pub struct SteamBackend;

#[async_trait]
impl Backend for SteamBackend {
    fn id(&self) -> &'static str {
        backends::STEAM
    }

    async fn apply(&self, online: bool) -> BackendOutcome {
        steam::apply_steam_status(online)
    }
}

/// Ids of the backends to apply in this tick.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    pub ids: Vec<&'static str>,
}

impl ApplyPlan {
    pub fn any(&self) -> bool {
        !self.ids.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(&id)
    }
}

/// Result for one backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendOutcome {
    /// Not selected in this tick.
    NotAttempted,
    /// Request sent. For Steam this only means the URI was handed to the running client;
    /// Steam gives no confirmation.
    Sent,
    /// Deliberately not applied (no token, Steam not running): no error, no retry.
    Skipped(String),
    /// Failed. `permanent` errors are not retried automatically. `retry_after`: the service
    /// asked not to be contacted again before this delay (rate limit); the scheduler holds the
    /// backend back until then instead of retrying on its own backoff.
    Failed {
        error: String,
        permanent: bool,
        retry_after: Option<Duration>,
    },
}

impl BackendOutcome {
    pub fn error(&self) -> Option<String> {
        match self {
            Self::Failed { error, .. } => Some(error.clone()),
            _ => None,
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Delay the service asked for before the next request (rate limit).
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Failed { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// Failed with an error a retry on the scheduler's own backoff may fix (not permanent,
    /// no delay given by the service).
    pub fn needs_retry(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                permanent: false,
                retry_after: None,
                ..
            }
        )
    }
}

/// Outcome per backend id, in the order the backends were applied.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyResult {
    pub outcomes: Vec<(&'static str, BackendOutcome)>,
}

impl ApplyResult {
    pub fn not_attempted() -> Self {
        Self::default()
    }

    /// Outcome for `id`; [`BackendOutcome::NotAttempted`] when it was not applied.
    pub fn get(&self, id: &str) -> &BackendOutcome {
        static NOT_ATTEMPTED: BackendOutcome = BackendOutcome::NotAttempted;
        self.outcomes
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, o)| o)
            .unwrap_or(&NOT_ATTEMPTED)
    }

    /// No backend failed (skipped and not attempted count as ok).
    pub fn all_ok(&self) -> bool {
        !self.outcomes.iter().any(|(_, o)| o.is_failed())
    }

    /// At least one backend failed with an error a retry may fix.
    pub fn needs_retry(&self) -> bool {
        self.outcomes.iter().any(|(_, o)| o.needs_retry())
    }
}

/// Apply presence to the backends selected in `plan`.
pub async fn apply_presence(
    backends: &[Box<dyn Backend>],
    work_mode: bool,
    plan: &ApplyPlan,
) -> ApplyResult {
    let online = !work_mode;
    let mut outcomes = Vec::new();
    for b in backends {
        if !plan.contains(b.id()) {
            continue;
        }
        let outcome = b.apply(online).await;
        log_outcome(b.id(), online, &outcome);
        outcomes.push((b.id(), outcome));
    }
    ApplyResult { outcomes }
}

fn log_outcome(backend: &str, online: bool, outcome: &BackendOutcome) {
    match outcome {
        BackendOutcome::NotAttempted => {}
        BackendOutcome::Sent => info!(
            target: "awayd::apply",
            backend,
            online,
            ok = true,
            "presence sent"
        ),
        BackendOutcome::Skipped(reason) => info!(
            target: "awayd::apply",
            backend,
            online,
            reason = %reason,
            "presence skipped"
        ),
        BackendOutcome::Failed {
            error,
            permanent,
            retry_after,
        } => warn!(
            target: "awayd::apply",
            message_id = "awayd_apply_fail",
            backend,
            online,
            ok = false,
            permanent,
            retry_after_secs = retry_after.map(|d| d.as_secs()),
            error = %error,
            "presence failed"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed {
        id: &'static str,
        outcome: BackendOutcome,
    }

    #[async_trait]
    impl Backend for Fixed {
        fn id(&self) -> &'static str {
            self.id
        }
        async fn apply(&self, _online: bool) -> BackendOutcome {
            self.outcome.clone()
        }
    }

    fn fixed(id: &'static str, outcome: BackendOutcome) -> Box<dyn Backend> {
        Box::new(Fixed { id, outcome })
    }

    #[tokio::test]
    async fn applies_only_planned_backends_in_order() {
        let list = vec![
            fixed("alpha", BackendOutcome::Sent),
            fixed("beta", BackendOutcome::Skipped("off".into())),
            fixed("gamma", BackendOutcome::Sent),
        ];
        let plan = ApplyPlan {
            ids: vec!["gamma", "alpha"],
        };
        let r = apply_presence(&list, false, &plan).await;
        assert_eq!(
            r.outcomes,
            vec![
                ("alpha", BackendOutcome::Sent),
                ("gamma", BackendOutcome::Sent)
            ]
        );
        assert_eq!(r.get("beta"), &BackendOutcome::NotAttempted);
        assert!(r.all_ok());
    }

    #[test]
    fn every_table_entry_has_an_implementation() {
        for info in awayd_core::backends::ALL {
            assert_eq!(build_backend(info, String::new()).id(), info.id);
        }
    }

    #[test]
    fn credential_normalization() {
        assert_eq!(normalize_credential(backends::STEAM, "  x "), "x");
        assert_eq!(
            normalize_credential(backends::DISCORD, " abc "),
            discord::normalize_token(" abc ")
        );
        assert_eq!(fingerprint(" abc "), fingerprint("abc"));
        assert_ne!(fingerprint("abc"), fingerprint("abd"));
    }
}
