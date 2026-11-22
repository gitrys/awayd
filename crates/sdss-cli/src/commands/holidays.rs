//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use sdss_core::config::paths;
use sdss_core::config::{self};
use sdss_ipc::SchedulerClient;

pub async fn list() -> Result<()> {
    let mut client = SchedulerClient::new();
    if !client.ping().await.unwrap_or(false) {
        println!(
            "daemon offline — cache dir: {}",
            paths::holidays_dir().display()
        );
        return Ok(());
    }
    // list_holiday_bundles not exposed on client yet — print cache dir
    println!("Holiday cache: {}", paths::holidays_dir().display());
    let cfg = config::load_config(&paths::config_file()).context("load config")?;
    let cc = cfg.schedule.country.to_uppercase();
    let (ready, y0, y1, fetched, msg) = client.holiday_data_status(&cc).await?;
    if ready {
        println!("{cc} years={y0}..{y1} updated={fetched}");
    } else {
        println!("{msg}");
    }
    Ok(())
}

pub async fn refresh(country: Option<String>) -> Result<()> {
    let cc = match country {
        Some(c) => c.to_uppercase(),
        None => config::load_config(&paths::config_file())
            .context("load config")?
            .schedule
            .country
            .to_uppercase(),
    };
    let mut client = SchedulerClient::new();
    let msg = client.refresh_holiday_data(&cc).await?;
    println!("{msg}");
    Ok(())
}
