# sdss — Steam / Discord Status Scheduler (Windows)

GPL-3.0-or-later. **Windows 11** portable build.

| Binary | Crate | Role |
|--------|-------|------|
| `sdssd` | `sdss-daemon` | Resident agent: scheduler + tray + named-pipe IPC |
| `sdss-settings` | `sdss-settings` | egui settings UI (on demand) |
| `sdss` | `sdss-cli` | CLI: doctor, autostart, override, validate |

Shared: **`sdss-core`**, IPC: **`sdss-ipc`** (JSON-RPC over `\\.\pipe\sdss-scheduler`).

## Build

```powershell
cargo build --release
```

Binaries: `target\release\{sdssd,sdss-settings,sdss}.exe`

Portable bundle:

```powershell
.\packaging\windows\build-portable.ps1
```

## Run

1. `sdssd.exe` — tray + scheduler (single process).
2. Settings from tray or `sdss-settings.exe`.
3. `sdss.exe install-autostart` for login autostart.

Config: `{exe_dir}/config/config.toml` when `portable.marker` is present.

## Logs

The daemon writes NDJSON lines to `{exe_dir}/state/logs/sdss.log` (rotated; default DEBUG+ capture).
View in **Settings → Logs** (live tail, search, level filters) or:

```powershell
.\sdss.exe logs --tail 100 --grep sent
.\sdss.exe doctor --pretty
```

`[advanced]` in `config.toml`: `log_min_level`, `log_max_bytes`, `log_max_files`.

See [docs/install-windows.md](docs/install-windows.md).

## Linux

An earlier Linux port exists but is not part of this repository yet.
