//! SPDX-License-Identifier: GPL-3.0-or-later
//! WMI push event when Discord.exe starts (re-apply presence).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};
use wmi::{COMLibrary, WMIConnection};

use crate::watchers::PokeGate;

const WQL: &str =
    "SELECT * FROM __InstanceCreationEvent WITHIN 2 WHERE TargetInstance ISA 'Win32_Process' AND TargetInstance.Name = 'Discord.exe'";

pub fn spawn(poke: Arc<PokeGate>) {
    std::thread::Builder::new()
        .name("sdss-discord-wmi".into())
        .spawn(move || {
            loop {
                match watch_loop(poke.clone()) {
                    Ok(()) => {}
                    Err(e) => warn!(target: "sdss::discord_watch", "wmi: {e}"),
                }
                std::thread::sleep(Duration::from_secs(10));
            }
        })
        .ok();
}

fn watch_loop(poke: Arc<PokeGate>) -> Result<(), String> {
    let com = COMLibrary::new().map_err(|e| e.to_string())?;
    let wmi = WMIConnection::new(com).map_err(|e| e.to_string())?;
    let mut stream = wmi
        .raw_notification::<HashMap<String, wmi::Variant>>(WQL)
        .map_err(|e| e.to_string())?;
    while let Some(event) = stream.next() {
        match event {
            Ok(_) => {
                info!(target: "sdss::discord_watch", "Discord.exe started");
                poke.poke_immediate();
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
