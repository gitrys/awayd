//! SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Result;
use awayd_ipc::SchedulerClient;

use crate::ttl::parse_ttl_seconds;
use crate::OverrideCmd;

pub async fn run(sub: OverrideCmd) -> Result<()> {
    let mut client = SchedulerClient::new();

    match sub {
        OverrideCmd::Set { mode, ttl } => {
            let secs = parse_ttl_seconds(&ttl).map_err(anyhow::Error::msg)?;
            let ok = client.set_override(&mode.to_lowercase(), secs).await?;
            println!("SetOverride: {ok} (ttl={secs}s)");
            if !ok {
                // Exit code 1; the daemon log names the reason (bad mode, config, state).
                anyhow::bail!("awayd rejected the override (see `awayctl logs --tail 20`)");
            }
        }
        OverrideCmd::Clear => {
            let ok = client.clear_override().await?;
            println!("ClearOverride: {ok}");
            if !ok {
                anyhow::bail!("awayd could not clear the override (see `awayctl logs --tail 20`)");
            }
        }
    }
    Ok(())
}
