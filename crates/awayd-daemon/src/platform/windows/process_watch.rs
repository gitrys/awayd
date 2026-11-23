//! SPDX-License-Identifier: GPL-3.0-or-later
//! Re-apply presence when the program of a switched-on backend starts (e.g. Discord.exe,
//! steam.exe).
//!
//! Every `advanced.process_watch_seconds` the PID list (`EnumProcesses`, about 0.03 ms) is
//! compared with the previous one; only new PIDs are opened to read their executable name.
//! A full Toolhelp snapshot with all names cost about 7.6 ms per check, the largest part of
//! awayd's idle CPU. A WMI subscription was used before that: its per-user memory quota is
//! shared with other programs (e.g. Logitech G HUB) and could be used up (0x8004106C).
//!
//! Only the programs of switched-on backends are watched (`backends::process_names_where`);
//! when there are none, or the interval is 0, the thread rests without waking up until the
//! settings change. The scheduler passes the current settings after every config load.

use std::collections::HashSet;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use awayd_core::backends::process_names_where;
use awayd_core::config::schema::AppConfig;
use tracing::{info, warn};

use super::process::{process_name, running_pids};
use crate::watchers::PokeGate;

/// One Discord start spawns several `Discord.exe` processes within seconds; poke once.
/// A poke re-applies all backends, so one window for all names is enough.
const COALESCE_WINDOW: Duration = Duration::from_secs(10);

/// What to watch and how often; `interval: None` = off.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WatchSettings {
    names: Vec<&'static str>,
    interval: Option<Duration>,
}

impl WatchSettings {
    fn from_config(app: &AppConfig) -> Self {
        let secs = app.advanced.process_watch_seconds;
        Self {
            names: process_names_where(|id| app.backend_enabled(id)),
            interval: (secs > 0).then(|| Duration::from_secs(secs)),
        }
    }

    fn active(&self) -> bool {
        !self.names.is_empty() && self.interval.is_some()
    }
}

struct Shared {
    settings: Mutex<Option<WatchSettings>>,
    changed: Condvar,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

/// Pass the settings of a loaded config to the watcher (no-op before `spawn`). Wakes the
/// watcher only when something changed.
pub fn configure(app: &AppConfig) {
    let Some(shared) = SHARED.get() else {
        return;
    };
    let next = WatchSettings::from_config(app);
    let mut current = shared.settings.lock().unwrap_or_else(|e| e.into_inner());
    if current.as_ref() != Some(&next) {
        info!(
            target: "awayd::process_watch",
            names = ?next.names,
            interval_s = next.interval.map_or(0, |d| d.as_secs()),
            "process watch settings"
        );
        *current = Some(next);
        shared.changed.notify_all();
    }
}

/// Leading-edge coalescing: fire for the first event, ignore further ones within `window`.
struct Coalescer {
    window: Duration,
    last: Option<Instant>,
}

impl Coalescer {
    fn new(window: Duration) -> Self {
        Self { window, last: None }
    }

    fn should_fire(&mut self, now: Instant) -> bool {
        if let Some(t) = self.last {
            if now.saturating_duration_since(t) < self.window {
                return false;
            }
        }
        self.last = Some(now);
        true
    }
}

/// PIDs in `now` that were not in `previous` (started since the last check).
fn new_pids<'a>(
    previous: &'a HashSet<u32>,
    now: &'a HashSet<u32>,
) -> impl Iterator<Item = u32> + 'a {
    now.difference(previous).copied()
}

/// First started process whose name is watched, as (name, PID).
fn first_watched_start(
    started: impl Iterator<Item = u32>,
    names: &[&str],
    name_of: impl Fn(u32) -> Option<String>,
) -> Option<(String, u32)> {
    started.into_iter().find_map(|pid| {
        let name = name_of(pid)?;
        names
            .iter()
            .any(|w| w.eq_ignore_ascii_case(&name))
            .then_some((name, pid))
    })
}

pub fn spawn(poke: Arc<PokeGate>) {
    let shared = SHARED
        .get_or_init(|| {
            Arc::new(Shared {
                settings: Mutex::new(None),
                changed: Condvar::new(),
            })
        })
        .clone();
    std::thread::Builder::new()
        .name("awayd-process-watch".into())
        .spawn(move || watch_loop(&shared, &poke))
        .ok();
}

fn watch_loop(shared: &Shared, poke: &PokeGate) {
    let mut coalescer = Coalescer::new(COALESCE_WINDOW);
    // `None` after start and after a rest: processes already running are not "started"
    // (the scheduler applies everything at start-up and on config changes anyway).
    let mut previous: Option<HashSet<u32>> = None;
    let mut failing = false;
    loop {
        // Rest without waking up until there is something to watch.
        let settings = {
            let mut guard = shared.settings.lock().unwrap_or_else(|e| e.into_inner());
            while !guard.as_ref().is_some_and(WatchSettings::active) {
                previous = None;
                guard = shared
                    .changed
                    .wait(guard)
                    .unwrap_or_else(|e| e.into_inner());
            }
            guard.clone().expect("active settings")
        };
        let interval = settings.interval.expect("active settings have an interval");

        match running_pids() {
            Ok(pids) => {
                if failing {
                    info!(target: "awayd::process_watch", "process list works again");
                    failing = false;
                }
                let now: HashSet<u32> = pids.into_iter().collect();
                if let Some(prev) = &previous {
                    let started =
                        first_watched_start(new_pids(prev, &now), &settings.names, process_name);
                    if let Some((name, pid)) = started {
                        if coalescer.should_fire(Instant::now()) {
                            info!(
                                target: "awayd::process_watch",
                                pid,
                                name = %name,
                                "{name} started"
                            );
                            // `poke_immediate`: only the immediate flag makes the tick
                            // re-apply when the last applied state already matches the
                            // target, which is exactly the case after Discord changed the
                            // status client-side.
                            poke.poke_immediate();
                        }
                    }
                }
                previous = Some(now);
            }
            // Logged once per failure streak (not every check).
            Err(e) if !failing => {
                warn!(
                    target: "awayd::process_watch",
                    error = %e,
                    "process list failed; program starts are not detected until it works again"
                );
                failing = true;
            }
            Err(_) => {}
        }

        // Sleep the interval, but wake at once when the settings change.
        let guard = shared.settings.lock().unwrap_or_else(|e| e.into_inner());
        let _ = shared
            .changed
            .wait_timeout_while(guard, interval, |s| s.as_ref() == Some(&settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn new_pids_are_only_the_added_ones() {
        let previous: HashSet<u32> = [10, 12].into();
        let now: HashSet<u32> = [10, 20].into();
        // 12 ended (not an event), 10 still running, 20 is new.
        assert_eq!(new_pids(&previous, &now).collect::<Vec<_>>(), vec![20]);
    }

    #[test]
    fn only_watched_names_count_as_starts() {
        let names = |pid: u32| match pid {
            1 => Some("explorer.exe".to_string()),
            2 => Some("discord.exe".to_string()),
            _ => None,
        };
        let hit = first_watched_start([1, 3, 2].into_iter(), &["Discord.exe"], names);
        assert_eq!(hit, Some(("discord.exe".to_string(), 2)));
        assert_eq!(
            first_watched_start([1, 3].into_iter(), &["Discord.exe"], names),
            None
        );
    }

    #[test]
    fn settings_follow_switched_on_services_and_interval() {
        let mut app = AppConfig::default();
        app.set_backend_enabled("discord", false);
        app.set_backend_enabled("steam", true);
        let s = WatchSettings::from_config(&app);
        assert_eq!(s.names, vec!["steam.exe"]);
        assert_eq!(s.interval, Some(Duration::from_secs(5)));
        assert!(s.active());

        app.set_backend_enabled("steam", false);
        assert!(
            !WatchSettings::from_config(&app).active(),
            "nothing to watch: rest"
        );

        app.set_backend_enabled("steam", true);
        app.advanced.process_watch_seconds = 0;
        assert!(
            !WatchSettings::from_config(&app).active(),
            "turned off: rest"
        );
    }

    #[test]
    fn coalescer_fires_once_per_window() {
        let mut c = Coalescer::new(Duration::from_secs(10));
        let t0 = Instant::now();
        assert!(c.should_fire(t0));
        assert!(!c.should_fire(t0 + Duration::from_millis(500)));
        assert!(!c.should_fire(t0 + Duration::from_secs(9)));
        assert!(c.should_fire(t0 + Duration::from_secs(10)));
        assert!(!c.should_fire(t0 + Duration::from_secs(11)));
    }

    /// Real process start between two PID lists (`ping.exe`, never touches Discord or the
    /// presence).
    #[test]
    fn real_process_start_is_detected() {
        let before: HashSet<u32> = running_pids().unwrap().into_iter().collect();
        let mut child = Command::new("ping")
            .args(["-n", "4", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let child_pid = child.id();
        let after: HashSet<u32> = running_pids().unwrap().into_iter().collect();
        let hit = first_watched_start(new_pids(&before, &after), &["PING.EXE"], process_name);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(hit.map(|(_, pid)| pid), Some(child_pid));
    }
}
