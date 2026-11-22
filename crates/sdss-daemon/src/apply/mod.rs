//! SPDX-License-Identifier: GPL-3.0-or-later
//! Apply presence to Discord + Steam independently (no short-circuit).

pub mod discord;
pub mod steam;

use tracing::warn;

use crate::work_window::Config;

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub discord_ok: bool,
    pub steam_ok: bool,
    pub discord_error: Option<String>,
    pub steam_error: Option<String>,
}

impl ApplyResult {
    pub fn all_ok(&self) -> bool {
        self.discord_ok && self.steam_ok
    }
}

/// Apply presence to Discord and Steam.
pub async fn apply_presence(cfg: &Config, work_mode: bool) -> ApplyResult {
    let online = !work_mode;
    let (discord_ok, discord_error) =
        match discord::patch_status(&cfg.discord_auth_token, online).await {
            Ok(_msg) => {
                tracing::info!(
                    target: "sdss::apply",
                    backend = "discord",
                    online = online,
                    ok = true,
                    "presence sent"
                );
                (true, None)
            }
            Err(e) => {
                warn!(
                    target: "sdss::apply",
                    message_id = "sdss_apply_fail",
                    backend = "discord",
                    online = online,
                    ok = false,
                    error = %e,
                    "presence failed"
                );
                (false, Some(e))
            }
        };
    let (steam_ok, steam_error) = match steam::apply_steam_status(online) {
        Ok(()) => {
            tracing::info!(
                target: "sdss::apply",
                backend = "steam",
                online = online,
                ok = true,
                "presence sent"
            );
            (true, None)
        }
        Err(e) => {
            warn!(
                target: "sdss::apply",
                message_id = "sdss_apply_fail",
                backend = "steam",
                online = online,
                ok = false,
                error = %e,
                "presence failed"
            );
            (false, Some(e))
        }
    };
    ApplyResult {
        discord_ok,
        steam_ok,
        discord_error,
        steam_error,
    }
}
