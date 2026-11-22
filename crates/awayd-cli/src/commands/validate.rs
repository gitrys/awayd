//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use awayd_core::config;
use awayd_core::config::paths;

/// Same checks as the daemon applies (schedule and vacations), so "config OK" means the
/// daemon will accept the file. Errors end with exit code 1.
pub fn validate() -> Result<()> {
    let path = paths::config_file();
    let cfg = config::load_config(&path)?;
    config::validate_app_config(&cfg).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    println!("config OK");
    Ok(())
}
