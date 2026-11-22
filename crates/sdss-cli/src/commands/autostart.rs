//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "SDSS";

pub fn install() -> Result<()> {
    let exe = sdss_core::launch::resolve_sdss_binary("sdssd");
    if !exe.is_file() {
        anyhow::bail!("sdssd.exe not found beside {}", std::env::current_exe()?.display());
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(RUN_KEY)?;
    run.set_value(VALUE_NAME, &exe.to_string_lossy().to_string())?;
    println!("autostart installed: {}", exe.display());
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run) = hkcu.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
        let _ = run.delete_value(VALUE_NAME);
    }
    println!("autostart removed");
    Ok(())
}
