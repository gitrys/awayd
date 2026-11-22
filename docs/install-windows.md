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
4. Complete the wizard: services (Steam on, Discord off by default; the Discord token is only
   asked for when you switch Discord on), country/region, work hours. Change the services later
   in **Settings → Scheduler → Services** or in `config.toml` (`[backends.discord]`,
   `[backends.steam]`, `enabled = true|false`).
5. Register autostart: `awayctl.exe install-autostart`

## Autostart

One `HKCU\Run` entry (value `awayd`, quoted path) starts `awayd.exe` at login (no admin).
Toggle it in **Settings → Scheduler → Start awayd at login**, or from the command line:

```powershell
awayctl.exe install-autostart
awayctl.exe uninstall-autostart
```

The settings window also shows when the entry points to another folder (e.g. after moving the
portable folder) or uses the old unquoted format, and offers to fix it.

## CLI

| Command | Purpose |
|---------|---------|
| `awayctl doctor` | Paths, daemon reachability, status (`--pretty` table, `--json` one JSON object) |
| `awayctl validate` | Validate `config.toml` |
| `awayctl override set invisible` | Temporary override |
| `awayctl override clear` | Clear override |

Exit codes: `0` = success. `1` = error, also when `doctor` finds the daemon not running or
reporting problems (`ok: no`, e.g. a missing Discord token while Discord is on), and when the
daemon rejects `override set` / `override clear`. The reason is printed on stderr, so
`awayctl doctor --json` keeps valid JSON on stdout.

## Notifications and gaming

Failure toasts are **suppressed** when Windows reports fullscreen D3D, presentation mode, busy, or quiet time (`SHQueryUserNotificationState`). Borderless games may still show toasts — add a Focus Assist rule in Windows Settings if needed.

