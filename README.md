# awayd

Away/online status scheduler for **Steam** and **Discord** on Windows.

awayd runs in the system tray and switches your status automatically: **invisible** during your
work hours, **online** outside of them. It knows public holidays, your vacations and manual
overrides ("stay invisible for 2 hours").

> [!WARNING]
> **Discord: use at your own risk.** awayd changes your Discord status through Discord's web API
> with **your user token** (the token of your normal Discord account) and a browser user agent.
> Automating a user account this way is very likely against Discord's Terms of Service
> ("self-bot") and may get your account restricted or banned. If you do not want that risk, do
> not enter a Discord token: awayd then only manages Steam.

awayd is an independent project. It is **not affiliated with, endorsed by or sponsored by Valve
Corporation (Steam) or Discord Inc.** "Steam" and "Discord" are trademarks of their respective
owners and are used here only to describe what the program works with.

## Features

- Work hours per weekday, public holidays (country and region), vacation ranges
- Manual override from the tray or CLI, either for a fixed time or until the next scheduled change
- Re-applies the status when Discord or Steam starts and after resume from standby
- Steam is only touched while it is running (awayd never starts Steam)
- Settings window, live log view, command-line client
- Portable: everything lives next to the executables

## Requirements

- Windows 11 (Windows 10 is untested)
- Steam client and/or Discord account (both optional)

## Install (portable)

1. Build the portable folder (see [Build](#build)) or take it from a release, and copy it anywhere,
   for example `C:\Tools\awayd-portable\`.
2. Start `awayd.exe`. A tray icon appears.
3. Open **Settings** from the tray menu (or run `awayd-settings.exe`) and complete the wizard:
   country/region, work hours and, optionally, the Discord token (see the warning above).
4. Start awayd automatically at login: tick **Settings → Scheduler → Start awayd at login**, or
   use the CLI:

   ```powershell
   .\awayctl.exe install-autostart     # adds HKCU\...\Run value "awayd"
   .\awayctl.exe uninstall-autostart   # removes it again
   ```

   If you move the folder, the settings window shows that the entry points to the old location
   and offers to update it.

More details: [docs/install-windows.md](docs/install-windows.md).

## Programs

| Program | Crate | Purpose |
|---|---|---|
| `awayd.exe` | `awayd-daemon` | Resident agent: scheduler, tray, named-pipe IPC |
| `awayd-settings.exe` | `awayd-settings` | Settings window (egui), opened on demand |
| `awayctl.exe` | `awayd-cli` | CLI: `doctor`, `logs`, `override`, `validate`, `holidays`, autostart |

Shared libraries: `awayd-core` (config, schedule logic) and `awayd-ipc` (JSON-RPC over the named
pipe `\\.\pipe\awayd`).

## Where data is stored

With `portable.marker` next to the executables (the default for the portable folder):

| What | Location |
|---|---|
| Settings | `config\config.toml` |
| State (override, last apply) | `state\state.json` |
| Logs (NDJSON, rotated) | `state\logs\awayd.log` |
| Holiday data cache | `data\holidays\<COUNTRY>.json` |
| Discord token | Windows Credential Manager, service `awayd`. Only if that fails: plain text in `state\discord_token.secret` |

Without `portable.marker` the same layout is used under `%APPDATA%\awayd`.

Logs can be viewed in **Settings → Logs** or with:

```powershell
.\awayctl.exe logs --tail 100 --grep sent
.\awayctl.exe doctor --pretty
```

## Build

Requires Rust 1.88 or newer (MSVC toolchain).

```powershell
cargo build --release                       # target\release\{awayd,awayd-settings,awayctl}.exe
.\packaging\windows\build-portable.ps1      # dist\awayd-portable\
cargo test --workspace -- --test-threads=1  # tests run serially (they share process env vars)
```

## Credits

- Public holiday data: [Nager.Date](https://date.nager.at), downloaded at runtime.

## License

Copyright (C) 2026 awayd contributors.

awayd is free software: you can redistribute it and/or modify it under the terms of the GNU
General Public License as published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version. See [LICENSE](LICENSE).
