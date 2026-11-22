//! SPDX-License-Identifier: GPL-3.0-or-later
//! Apply presence to Discord + Steam independently (no short-circuit), only for the backends
//! the scheduler selected.

pub mod discord;
pub mod steam;

use tracing::{info, warn};

use crate::work_window::Config;

/// Which backends to apply in this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApplyPlan {
    pub discord: bool,
    pub steam: bool,
}

impl ApplyPlan {
    pub fn any(&self) -> bool {
        self.discord || self.steam
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
    /// Failed. `permanent` errors are not retried automatically.
    Failed { error: String, permanent: bool },
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

    pub fn needs_retry(&self) -> bool {
        matches!(
            self,
            Self::Failed {
                permanent: false,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResult {
    pub discord: BackendOutcome,
    pub steam: BackendOutcome,
}

impl ApplyResult {
    pub fn not_attempted() -> Self {
        Self {
            discord: BackendOutcome::NotAttempted,
            steam: BackendOutcome::NotAttempted,
        }
    }

    /// No backend failed (skipped and not attempted count as ok).
    pub fn all_ok(&self) -> bool {
        !self.discord.is_failed() && !self.steam.is_failed()
    }

    /// At least one backend failed with an error a retry may fix.
    pub fn needs_retry(&self) -> bool {
        self.discord.needs_retry() || self.steam.needs_retry()
    }
}

/// Apply presence to the backends selected in `plan`.
pub async fn apply_presence(cfg: &Config, work_mode: bool, plan: ApplyPlan) -> ApplyResult {
    let online = !work_mode;
    let discord = if plan.discord {
        apply_discord(&cfg.discord_auth_token, online).await
    } else {
        BackendOutcome::NotAttempted
    };
    let steam = if plan.steam {
        steam::apply_steam_status(online)
    } else {
        BackendOutcome::NotAttempted
    };
    log_outcome("discord", online, &discord);
    log_outcome("steam", online, &steam);
    ApplyResult { discord, steam }
}

async fn apply_discord(token: &str, online: bool) -> BackendOutcome {
    if token.trim().is_empty() {
        return BackendOutcome::Skipped("no discord token configured".into());
    }
    match discord::patch_status(token, online).await {
        Ok(_) => BackendOutcome::Sent,
        Err(e) => BackendOutcome::Failed {
            error: e.to_string(),
            permanent: e.is_permanent(),
        },
    }
}

fn log_outcome(backend: &str, online: bool, outcome: &BackendOutcome) {
    match outcome {
        BackendOutcome::NotAttempted => {}
        BackendOutcome::Sent => info!(
            target: "sdss::apply",
            backend,
            online,
            ok = true,
            "presence sent"
        ),
        BackendOutcome::Skipped(reason) => info!(
            target: "sdss::apply",
            backend,
            online,
            reason = %reason,
            "presence skipped"
        ),
        BackendOutcome::Failed { error, permanent } => warn!(
            target: "sdss::apply",
            message_id = "sdss_apply_fail",
            backend,
            online,
            ok = false,
            permanent,
            error = %error,
            "presence failed"
        ),
    }
}
