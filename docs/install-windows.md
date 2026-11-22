# awayd — Windows install (portable)

awayd for **Windows 11**. One resident process (`awayd.exe`) runs the scheduler and system tray.

## Portable folder layout

```
awayd-portable/
  awayd.exe            # resident agent (scheduler + tray)
  awayd-settings.exe   # settings GUI (on demand)
  awayctl.exe          # CLI
  portable.marker
  config/
    config.toml
  state/
    state.json
  data/
    holidays/
```

Place `portable.marker` next to the EXEs (or set `AWAYD_PORTABLE=1`) so config/state/data live **beside the binaries**.

## Build

```powershell
cargo build --release
# or
.\packaging\windows\build-portable.ps1
```

## First run

1. Copy the portable folder anywhere (e.g. `F:\apps\awayd-portable\`).
2. Run `awayd.exe` — tray icon appears.
3. Open **Settings** from the tray menu (or run `awayd-settings.exe`).
4. Complete the wizard: Discord token, Germany/Bundesland, work hours.
5. Register autostart: `awayctl.exe install-autostart`

## Autostart

One `HKCU\Run` entry starts `awayd.exe` at login (no admin):

```powershell
awayctl.exe install-autostart
awayctl.exe uninstall-autostart
```

## CLI

| Command | Purpose |
|---------|---------|
| `awayctl doctor` | Paths, daemon reachability, status |
| `awayctl validate` | Validate `config.toml` |
| `awayctl override set invisible` | Temporary override |
| `awayctl override clear` | Clear override |

## Notifications and gaming

Failure toasts are **suppressed** when Windows reports fullscreen D3D, presentation mode, busy, or quiet time (`SHQueryUserNotificationState`). Borderless games may still show toasts — add a Focus Assist rule in Windows Settings if needed.

## Linux

An earlier Linux port exists but is not part of this repository yet.
