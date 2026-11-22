# sdss — awayd (Windows)

GPL-3.0-or-later. **Windows 11** portable build.

| Binary | Crate | Role |
|--------|-------|------|
| `awayd` | `awayd-daemon` | Resident agent: scheduler + tray + named-pipe IPC |
| `awayd-settings` | `awayd-settings` | egui settings UI (on demand) |
| `awayctl` | `awayd-cli` | CLI: doctor, autostart, override, validate |

Shared: **`awayd-core`**, IPC: **`awayd-ipc`** (JSON-RPC over `\\.\pipe\awayd`).

## Build

```powershell
cargo build --release
```

Binaries: `target\release\{awayd,awayd-settings,awayctl}.exe`

Portable bundle:

```powershell
.\packaging\windows\build-portable.ps1
```

## Run

1. `awayd.exe` — tray + scheduler (single process).
2. Settings from tray or `awayd-settings.exe`.
3. `awayctl.exe install-autostart` for login autostart.

Config: `{exe_dir}/config/config.toml` when `portable.marker` is present.

## Logs

The daemon writes NDJSON lines to `{exe_dir}/state/logs/awayd.log` (rotated; default DEBUG+ capture).
View in **Settings → Logs** (live tail, search, level filters) or:

```powershell
.\awayctl.exe logs --tail 100 --grep sent
.\awayctl.exe doctor --pretty
```

`[advanced]` in `config.toml`: `log_min_level`, `log_max_bytes`, `log_max_files`.

See [docs/install-windows.md](docs/install-windows.md).

## Linux

An earlier Linux port exists but is not part of this repository yet.
