//! SPDX-License-Identifier: GPL-3.0-or-later
//! Steam friends status via steam:// URIs.

pub fn apply_steam_status(online: bool) -> Result<(), String> {
    crate::platform::windows::steam::apply_steam_status(online)
}
