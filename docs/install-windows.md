# SDSS — Windows install (portable)

Steam / Discord Status Scheduler for **Windows 11**. One resident process (`sdssd.exe`) runs the scheduler and system tray.

## Portable folder layout

```
sdss-portable/
  sdssd.exe           # resident agent (scheduler + tray)
  sdss-settings.exe   # settings GUI (on demand)
  sdss.exe            # CLI
  portable.marker
  config/
    config.toml
  state/
    state.json
  data/
    holidays/
```

Place `portable.marker` next to the EXEs (or set `SDSS_PORTABLE=1`) so config/state/data live **beside the binaries**.

## Build

```powershell
cargo build --release
# or
.\packaging\windows\build-portable.ps1
```

## First run

1. Copy the portable folder anywhere (e.g. `F:\apps\sdss-portable\`).
2. Run `sdssd.exe` — tray icon appears.
3. Open **Settings** from the tray menu (or run `sdss-settings.exe`).
4. Complete the wizard: Discord token, Germany/Bundesland, work hours.
5. Register autostart: `sdss.exe install-autostart`

## Autostart

One `HKCU\Run` entry starts `sdssd.exe` at login (no admin):

```powershell
sdss.exe install-autostart
sdss.exe uninstall-autostart
```

## CLI

| Command | Purpose |
|---------|---------|
| `sdss doctor` | Paths, daemon reachability, status |
| `sdss validate` | Validate `config.toml` |
| `sdss override set invisible` | Temporary override |
| `sdss override clear` | Clear override |

## Notifications and gaming

Failure toasts are **suppressed** when Windows reports fullscreen D3D, presentation mode, busy, or quiet time (`SHQueryUserNotificationState`). Borderless games may still show toasts — add a Focus Assist rule in Windows Settings if needed.

## Linux

An earlier Linux port exists but is not part of this repository yet.
