//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use awayd_core::autostart;

pub fn install() -> Result<()> {
    let exe = awayd_core::launch::resolve_awayd_binary("awayd");
    autostart::install(&exe)?;
    println!("autostart installed: {}", autostart::command_for(&exe));
    Ok(())
}

pub fn uninstall() -> Result<()> {
    if autostart::uninstall()? {
        println!("autostart removed");
    } else {
        println!("autostart was not set");
    }
    Ok(())
}
