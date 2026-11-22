//! SPDX-License-Identifier: GPL-3.0-or-later
//! Serialized read-modify-write of `state.json`.

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use sdss_core::config::{self, state::AppState};

/// One lock for every load → modify → save of `state.json` in this process (scheduler tick,
/// IPC/tray override). Without it, a tick that loaded the state before a slow apply wrote it
/// back afterwards and dropped an override set in between.
static STATE_LOCK: Mutex<()> = Mutex::new(());

/// Load `state.json`, apply `f`, save it, all under [`STATE_LOCK`]; returns the saved state.
pub fn update_state(path: &Path, f: impl FnOnce(&mut AppState)) -> Result<AppState> {
    // The lock guards no data, so a panic in another holder cannot leave it inconsistent.
    let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut st = config::load_state(path)?;
    f(&mut st);
    config::save_state(path, &st)?;
    Ok(st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdss_core::config::state::{LastApplyRecord, OverrideRecord};

    fn last_apply(at: &str) -> LastApplyRecord {
        LastApplyRecord {
            ok: true,
            work_mode: false,
            source: "schedule".into(),
            at: at.into(),
            discord_error: None,
            steam_error: None,
        }
    }

    fn override_rec() -> OverrideRecord {
        OverrideRecord {
            version: 1,
            work_mode: false,
            expires_at: "2099-01-01T00:00:00".into(),
        }
    }

    /// The #1 sequence: a tick has loaded the state (no override), the user sets an
    /// override while the apply runs, then the tick records `last_apply`.
    #[test]
    fn tick_keeps_override_set_during_apply() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        config::save_state(&path, &AppState::default()).unwrap();

        let tick_view = config::load_state(&path).unwrap();
        assert!(tick_view.r#override.is_none());

        update_state(&path, |st| st.r#override = Some(override_rec())).unwrap();
        let after_tick =
            update_state(&path, |st| st.last_apply = Some(last_apply("tick"))).unwrap();

        assert!(
            after_tick.r#override.is_some(),
            "override must survive the tick"
        );
        let on_disk = config::load_state(&path).unwrap();
        assert!(on_disk.r#override.is_some());
        assert_eq!(on_disk.last_apply.unwrap().at, "tick");
    }

    /// Concurrent read-modify-write must not lose updates: a counter kept in
    /// `last_apply.at` ends at threads × iterations.
    #[test]
    fn concurrent_updates_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        config::save_state(&path, &AppState::default()).unwrap();

        const THREADS: usize = 4;
        const ITERATIONS: usize = 25;
        std::thread::scope(|s| {
            for _ in 0..THREADS {
                s.spawn(|| {
                    for _ in 0..ITERATIONS {
                        update_state(&path, |st| {
                            let n: usize = st
                                .last_apply
                                .as_ref()
                                .and_then(|r| r.at.parse().ok())
                                .unwrap_or(0);
                            st.last_apply = Some(last_apply(&(n + 1).to_string()));
                        })
                        .unwrap();
                    }
                });
            }
        });

        let st = config::load_state(&path).unwrap();
        assert_eq!(
            st.last_apply.unwrap().at,
            (THREADS * ITERATIONS).to_string()
        );
    }
}
