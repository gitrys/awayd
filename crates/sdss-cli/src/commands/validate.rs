//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use sdss_core::config::paths;
use sdss_core::config;
use sdss_core::schedule::validate_schedule;

pub fn validate() -> Result<()> {
    let cfg = config::load_config(&paths::config_file())?;
    validate_schedule(&cfg.schedule).map_err(|e| anyhow::anyhow!(e))?;
    println!("config OK");
    Ok(())
}
