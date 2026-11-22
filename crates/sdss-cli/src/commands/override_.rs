//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use sdss_ipc::SchedulerClient;

use crate::ttl::parse_ttl_seconds;
use crate::OverrideCmd;

pub async fn run(sub: OverrideCmd) -> Result<()> {
    let mut client = SchedulerClient::new();

    match sub {
        OverrideCmd::Set { mode, ttl } => {
            let secs = parse_ttl_seconds(&ttl).map_err(anyhow::Error::msg)?;
            let ok = client
                .set_override(&mode.to_lowercase(), secs)
                .await?;
            println!("SetOverride: {ok} (ttl={secs}s)");
        }
        OverrideCmd::Clear => {
            let ok = client.clear_override().await?;
            println!("ClearOverride: {ok}");
        }
    }
    Ok(())
}
