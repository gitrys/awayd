//! SPDX-License-Identifier: GPL-3.0-or-later

const STEAM_ONLINE: &str = "steam://friends/status/online";
const STEAM_INVISIBLE: &str = "steam://friends/status/invisible";

pub fn apply_steam_status(online: bool) -> Result<(), String> {
    let uri = if online {
        STEAM_ONLINE
    } else {
        STEAM_INVISIBLE
    };
    open::that(uri).map_err(|e| format!("open steam uri: {e}"))
}
