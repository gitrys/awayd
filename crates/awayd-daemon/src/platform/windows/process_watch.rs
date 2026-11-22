//! SPDX-License-Identifier: GPL-3.0-or-later
//! Re-apply presence when the program of a backend starts (e.g. Discord.exe, steam.exe).
//!
//! A Toolhelp snapshot every 2 s (the latency of the former `WITHIN 2` WMI query) is compared
//! with the previous one; a new PID of a watched name pokes the scheduler. This replaced a WMI
//! process-creation subscription: WMI polling subscriptions share a per-user memory quota
//! with other programs (e.g. Logitech G HUB polls the whole process list), and once it was
//! used up a new subscription failed with 0x8004106C, so starts were no longer detected.
//!
//! The process names of all backends in `awayd_core::backends::ALL` are watched, also of
//! switched-off ones; a tick applies only the active backends. Steam is watched because the
//! Steam backend is skipped while Steam is not running and must be applied once it starts.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use awayd_core::backends::all_process_names;
use tracing::{info, warn};

use super::process::running_processes;
use crate::watchers::PokeGate;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// One Discord start spawns several `Discord.exe` processes within seconds; poke once.
/// A poke re-applies all backends, so one window for all names is enough.
const COALESCE_WINDOW: Duration = Duration::from_secs(10);

/// Watched processes of a snapshot (name compared case-insensitively).
fn watched_processes<'a>(
    snapshot: &'a [(String, u32)],
    watched: &[&str],
) -> Vec<&'a (String, u32)> {
    snapshot
        .iter()
        .filter(|(name, _)| watched.iter().any(|w| w.eq_ignore_ascii_case(name)))
        .collect()
}

/// Watched processes whose PID was not in the previous snapshot, i.e. started since then
/// (same meaning as a WMI process-creation event).
fn started<'a>(previous: &HashSet<u32>, now: &[&'a (String, u32)]) -> Vec<&'a (String, u32)> {
    now.iter()
        .copied()
        .filter(|(_, pid)| !previous.contains(pid))
        .collect()
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

pub fn spawn(poke: Arc<PokeGate>) {
    std::thread::Builder::new()
        .name("awayd-process-watch".into())
        .spawn(move || watch_loop(&poke))
        .ok();
}

fn watch_loop(poke: &PokeGate) {
    let watched = all_process_names();
    let mut coalescer = Coalescer::new(COALESCE_WINDOW);
    // `None` until the first snapshot: processes already running at start-up are not
    // "started" (the scheduler applies everything at start-up anyway).
    let mut previous: Option<HashSet<u32>> = None;
    let mut failing = false;
    loop {
        match running_processes() {
            Ok(snapshot) => {
                if failing {
                    info!(target: "awayd::process_watch", "process snapshot works again");
                    failing = false;
                }
                let now = watched_processes(&snapshot, &watched);
                if let Some(prev) = &previous {
                    if let Some((name, pid)) = started(prev, &now).first() {
                        if coalescer.should_fire(Instant::now()) {
                            info!(
                                target: "awayd::process_watch",
                                pid = *pid,
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
                previous = Some(now.iter().map(|(_, pid)| *pid).collect());
            }
            // Logged once per failure streak (not every poll).
            Err(e) if !failing => {
                warn!(
                    target: "awayd::process_watch",
                    error = %e,
                    "process snapshot failed; program starts are not detected until it works again"
                );
                failing = true;
            }
            Err(_) => {}
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn p(name: &str, pid: u32) -> (String, u32) {
        (name.to_string(), pid)
    }

    #[test]
    fn only_watched_names_are_considered() {
        let snap = vec![
            p("discord.exe", 10),
            p("explorer.exe", 11),
            p("Steam.exe", 12),
        ];
        let w = watched_processes(&snap, &["Discord.exe", "steam.exe"]);
        let pids: Vec<u32> = w.iter().map(|(_, pid)| *pid).collect();
        assert_eq!(pids, vec![10, 12]);
    }

    #[test]
    fn new_pids_are_starts_and_known_or_ended_ones_are_not() {
        let previous: HashSet<u32> = [10, 12].into();
        let snap = vec![p("Discord.exe", 10), p("Discord.exe", 20)];
        let now = watched_processes(&snap, &["Discord.exe", "steam.exe"]);
        // 12 ended (not an event), 10 still running, 20 is new (e.g. Discord restarted
        // while a helper process kept running).
        let s = started(&previous, &now);
        assert_eq!(s, vec![&p("Discord.exe", 20)]);
        assert!(started(&[10, 20].into(), &now).is_empty());
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

    /// Real process start between two snapshots (`ping.exe`, never touches Discord or the
    /// presence). Needs no WMI, so no quota can make it fail.
    #[test]
    fn real_process_start_is_detected() {
        let watched = ["PING.EXE"];
        let before = running_processes().unwrap();
        let previous: HashSet<u32> = watched_processes(&before, &watched)
            .iter()
            .map(|(_, pid)| *pid)
            .collect();
        let mut child = Command::new("ping")
            .args(["-n", "4", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let child_pid = child.id();
        let after = running_processes().unwrap();
        let now = watched_processes(&after, &watched);
        let s = started(&previous, &now);
        let _ = child.kill();
        let _ = child.wait();
        assert!(
            s.iter()
                .any(|(name, pid)| *pid == child_pid && name.eq_ignore_ascii_case("ping.exe")),
            "ping {child_pid} not detected: {s:?}"
        );
    }
}
