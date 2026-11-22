# awayd

Away/online status scheduler for **Steam** and **Discord** on Windows.

awayd runs in the system tray and switches your status automatically: **invisible** during your
work hours, **online** outside of them. It knows public holidays, your vacations and manual
overrides ("stay invisible for 2 hours").

> [!WARNING]
> **Discord: use at your own risk.** awayd changes your Discord status through Discord's web API
> with **your user token** (the token of your normal Discord account) and a browser user agent.
> Automating a user account this way is very likely against Discord's Terms of Service
> ("self-bot") and may get your account restricted or banned. Discord is therefore **switched
> off** in a new installation; switch it on (Settings → Scheduler → Services) only if you accept
> that risk.

awayd is an independent project. It is **not affiliated with, endorsed by or sponsored by Valve
Corporation (Steam) or Discord Inc.** "Steam" and "Discord" are trademarks of their respective
owners and are used here only to describe what the program works with.

## Features

- Choose the services awayd manages: Steam (on by default) and Discord (off by default)
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

1. Download `awayd-vX.Y.Z-windows-x64-portable.zip` from the
   [releases](https://github.com/gitrys/awayd/releases) (optionally [verify it](#verify-downloads))
   or build it yourself (see [Build](#build)), and extract it anywhere, for example
   `C:\Tools\awayd\`.
2. Start `awayd.exe`. A tray icon appears.

   The executables are not code-signed yet, so Windows SmartScreen may show "Windows protected
   your PC" on the first start. Choose **More info → Run anyway** if you trust the download
   (verified checksum/signature).
3. Open **Settings** from the tray menu (or run `awayd-settings.exe`) and complete the wizard:
   services, country/region and work hours. The Discord token is only asked for when you switch
   Discord on (see the warning above).

   The services can be changed later in **Settings → Scheduler → Services** or in
   `config\config.toml`:

   ```toml
   [backends.discord]
   enabled = false

   [backends.steam]
   enabled = true
   ```

   A service without entry counts as switched on (configurations from 0.1.0 keep both).
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
| Credentials (Discord token) | Windows Credential Manager, service `awayd`, user = service id (`discord`). Only if that fails: plain text in `state\credentials\<id>.secret` |

Without `portable.marker` the same layout is used under `%APPDATA%\awayd`.

Logs can be viewed in **Settings → Logs** or with:

```powershell
.\awayctl.exe logs --tail 100 --grep sent
.\awayctl.exe doctor --pretty
```

## Verify downloads

Every release contains `SHA256SUMS.txt` and a [minisign](https://jedisct1.github.io/minisign/)
signature (`*.zip.minisig`) made with the key in [`minisign.pub`](minisign.pub).

```powershell
# Checksum: must match the line in SHA256SUMS.txt
(Get-FileHash -Algorithm SHA256 .\awayd-vX.Y.Z-windows-x64-portable.zip).Hash.ToLower()

# Signature (minisign, or `rsign verify` from `cargo install rsign2`)
minisign -Vm awayd-vX.Y.Z-windows-x64-portable.zip -p minisign.pub
```

## Build

Requires Rust 1.88 or newer (MSVC toolchain, Windows SDK for the icon/version resources).

```powershell
cargo build --release                       # target\release\{awayd,awayd-settings,awayctl}.exe
.\packaging\windows\build-portable.ps1      # dist\awayd-portable\
cargo test --workspace -- --test-threads=1  # tests run serially (they share process env vars)
```

### Releasing (maintainers)

1. Update the version in `Cargo.toml` and add a `## [X.Y.Z]` section to `CHANGELOG.md`.
2. Push the tag `vX.Y.Z`. The `release` workflow runs the tests, builds the portable ZIP with
   third-party licenses (`cargo about`, see `about.toml`) and `SHA256SUMS.txt`, and creates a
   **draft** release. A manual run of the workflow is a dry run (bundle as workflow artifact only).
3. Sign locally: `.\packaging\windows\sign-release.ps1 -Tag vX.Y.Z` (verifies the checksum, signs
   with the minisign secret key, uploads the `.minisig`).
4. Review the draft and publish it.

## Security

See [SECURITY.md](SECURITY.md) for how the Discord token is stored, what awayd sends over the
network and how to report vulnerabilities privately.

## Credits

- Public holiday data: [Nager.Date](https://date.nager.at), downloaded at runtime.

## License

Copyright (C) 2026 awayd contributors.

awayd is free software: you can redistribute it and/or modify it under the terms of the GNU
General Public License as published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version. See [LICENSE](LICENSE).
