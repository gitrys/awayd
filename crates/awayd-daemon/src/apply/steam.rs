//! SPDX-License-Identifier: GPL-3.0-or-later
//! Steam friends status via steam:// URIs.

use super::BackendOutcome;

const STEAM_EXE: &str = "steam.exe";

/// Hand the status URI to Steam, but only while the client runs: a `steam://` URI would
/// otherwise start Steam. The process watcher pokes the scheduler when
/// Steam starts later.
pub fn apply_steam_status(online: bool) -> BackendOutcome {
    match crate::platform::windows::process::is_process_running(STEAM_EXE) {
        Ok(true) => {}
        Ok(false) => return BackendOutcome::Skipped("steam is not running".into()),
        Err(e) => {
            return BackendOutcome::Failed {
                error: format!("steam process check: {e}"),
                permanent: false,
                retry_after: None,
            }
        }
    }
    match crate::platform::windows::steam::apply_steam_status(online) {
        Ok(()) => BackendOutcome::Sent,
        Err(e) => BackendOutcome::Failed {
            error: e,
            permanent: false,
            retry_after: None,
        },
    }
}
