//! SPDX-License-Identifier: GPL-3.0-or-later
//! WMI push event when Discord.exe or steam.exe starts (re-apply presence).
//!
//! One WMI subscription for both names: event registrations count against a system-wide WMI
//! quota. Steam is watched because the Steam backend is skipped while Steam is not running
//! and must be applied once it starts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tracing::{info, warn};
use wmi::{COMLibrary, WMIConnection};

use crate::watchers::PokeGate;

const PROCESS_NAMES: &[&str] = &["Discord.exe", "steam.exe"];

/// One Discord start spawns several `Discord.exe` processes within seconds; poke once.
/// A poke re-applies both backends, so one window for all names is enough.
const COALESCE_WINDOW: Duration = Duration::from_secs(10);

fn wql(process_names: &[&str]) -> String {
    let names = process_names
        .iter()
        .map(|n| format!("TargetInstance.Name = '{n}'"))
        .collect::<Vec<_>>()
        .join(" OR ");
    format!(
        "SELECT * FROM __InstanceCreationEvent WITHIN 2 WHERE TargetInstance ISA 'Win32_Process' AND ({names})"
    )
}

/// `TargetInstance` is an embedded WMI object. `HashMap<String, wmi::Variant>` cannot
/// represent it, so every event used to fail to deserialize.
#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_Process", rename_all = "PascalCase")]
struct Process {
    process_id: u32,
    name: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct NewProcessEvent {
    target_instance: Process,
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
        .name("awayd-discord-wmi".into())
        .spawn(move || {
            let mut coalescer = Coalescer::new(COALESCE_WINDOW);
            loop {
                match watch_loop(poke.clone(), &mut coalescer) {
                    Ok(()) => {}
                    Err(e) => warn!(target: "awayd::discord_watch", "wmi: {e}"),
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .ok();
}

fn watch_loop(poke: Arc<PokeGate>, coalescer: &mut Coalescer) -> Result<(), String> {
    let com = COMLibrary::new().map_err(|e| e.to_string())?;
    let wmi = WMIConnection::new(com).map_err(|e| e.to_string())?;
    let stream = wmi
        .raw_notification::<NewProcessEvent>(wql(PROCESS_NAMES))
        .map_err(|e| e.to_string())?;
    for event in stream {
        let event = event.map_err(|e| e.to_string())?;
        if coalescer.should_fire(Instant::now()) {
            let p = &event.target_instance;
            info!(
                target: "awayd::discord_watch",
                pid = p.process_id,
                name = %p.name,
                "{} started",
                p.name
            );
            // `poke_immediate` (not `poke_debounced`): only the immediate flag makes the
            // tick re-apply when the last applied state already matches the target, which
            // is exactly the case after Discord changed the status client-side.
            poke.poke_immediate();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    #[test]
    fn wql_watches_discord_and_steam() {
        assert_eq!(
            wql(PROCESS_NAMES),
            "SELECT * FROM __InstanceCreationEvent WITHIN 2 WHERE TargetInstance ISA 'Win32_Process' \
             AND (TargetInstance.Name = 'Discord.exe' OR TargetInstance.Name = 'steam.exe')"
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

    /// Same query and event type as production, for `ping.exe` instead of Discord
    /// (starts only `ping`, never touches Discord or the presence).
    #[test]
    fn typed_event_deserializes_real_process_start() {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let run = || -> Result<(u32, String), String> {
                let com = COMLibrary::new().map_err(|e| e.to_string())?;
                let wmi = WMIConnection::new(com).map_err(|e| e.to_string())?;
                let mut stream = wmi
                    // Two names, like production: also checks the OR syntax against WMI.
                    .raw_notification::<NewProcessEvent>(wql(&[
                        "awayd-not-running.exe",
                        "PING.EXE",
                    ]))
                    .map_err(|e| e.to_string())?;
                Command::new("ping")
                    .args(["-n", "6", "127.0.0.1"])
                    .stdout(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let ev = stream
                    .next()
                    .ok_or("stream ended")?
                    .map_err(|e| e.to_string())?;
                Ok((ev.target_instance.process_id, ev.target_instance.name))
            };
            let _ = tx.send(run());
        });
        let (pid, name) = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("no WMI event within 30 s")
            .expect("event must deserialize");
        assert!(pid > 0);
        assert!(name.eq_ignore_ascii_case("ping.exe"), "name: {name}");
    }
}
