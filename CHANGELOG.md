# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Tray Settings… dialog (PySide6):** edit Discord token, work schedule (`QTimeEdit` + weekday toggles), Bundesland, named vacations, and presence mode in one place. Save writes the env file atomically (temp + rename, fail closed), preserves the existing token when the field is left blank, then sends a `settings_saved` poke so the worker reloads and reapplies without restart.
- **Named vacations in `vacations.json`:** vacations now live at `$XDG_CONFIG_HOME/steam-discord-status-scheduler/vacations.json` (`schema_version: 1`, atomic save). The Settings dialog manages add/remove/rename; legacy `SDSS_VACATION` / `vacationRanges` / `vacation` env keys are no longer read.
- **`SDSS_MODE` (`schedule` | `network`):** persisted in the env file and exposed via both the tray Mode submenu and the Settings dialog. Reading the value is now wired through the config reload path. `network` is a stub today; selecting it changes how manual overrides expire (see below).
- **Network-mode override TTL:** `steam-discord-status-scheduler override set …` uses a wall-clock TTL while `SDSS_MODE=network`, controlled by `SDSS_OVERRIDE_NETWORK_MAX_HOURS` (default `8`). Schedule mode continues to expire at the next schedule flip.
- **Config reload under systemd:** `load_dotenv_for_app()` now uses `override=True`, so edits made via the Settings dialog or by hand are honored on the next worker cycle even when systemd injects `EnvironmentFile=` at startup.
- **Steam D-Bus client delay:** after `com.steampowered.*` names appear, wait `SDSS_STEAM_CLIENT_DELAY_SEC` (default 10s) before poking so `steam://` apply runs once Steam is ready.
- **Fix tray poke not waking worker:** `wait_until_deadline` now notices `poke` during long sleep slices (was up to 60s late); coalesce defer path waits on poke too.
- **Steam apply after D-Bus:** if session D-Bus saw Steam but `pgrep -x steam` has not matched yet, still run `xdg-open` for that poke.
- **Steam apply without launching client:** by default, `xdg-open steam://…` runs only if a Steam process is already running (`SDSS_STEAM_LAUNCH_IF_CLOSED=0`); client-start poke applies when you open Steam later. Set `SDSS_STEAM_LAUNCH_IF_CLOSED=1` for the previous always-open behavior.
- **Tray suspend/resume and client-start poke:** logind `PrepareForSleep` / session `Unlock` and session D-Bus `NameOwnerChanged` (Steam/Discord) trigger an immediate `run_once` via a shared poke event (interruptible scheduler sleep fixes monotonic-timer gap after resume). Optional `SDSS_ON_SUSPEND` pre-sleep apply (`noop`|`online`|`invisible`|`last`, best-effort timeout). Optional process poll fallback. Env: `SDSS_POKE_GLOBAL_MIN_SEC`, `SDSS_REAPPLY_ON_CLIENT_START`, `SDSS_CLIENT_DETECT`, `SDSS_*_DBUS_NAMES`, etc. `doctor` reports watcher status and last poke reason when tray has run.
- **Tray status indicator:** monochrome rounded-square hub in `tray.svg` with a **corner dot** (green online, red invisible, grey after a failed apply) and a **blue ring** while `source=manual_override`.
- **`RunOnceResult`:** `run_once` returns apply outcome plus effective `work_mode` and `source` (CLI exit codes, tray snapshots).
- **`steam-discord-status-scheduler tray`:** resident tray using **PySide6** (`QSystemTrayIcon` + context menu: **Edit env file…**, **Override status** submenu (Online / Invisible / Clear override), **Apply now**, **Quit**); background loop runs `run_once` then sleeps until the next schedule flip or override expiry (capped). Optional extra: **`pip install -e '.[tray]'`** (`pyproject` group `tray` = PySide6). Tray SVG ships under `steam_discord_status_scheduler/icons/` for wheel installs.
- **`next_wakeup.seconds_until_next_wakeup`:** shared helper for tray sleep duration.
- **systemd installer:** writes `steam-discord-status-scheduler-tray.service` only; removes legacy oneshot + timer units from older installs so presence is not applied twice.
- **`doctor`:** warns when the tray service and **legacy** repeat/boundary timers are both enabled (`systemd_tray_timer_conflict` in `--json` output).
- **`steam-discord-status-scheduler override`:** `status` (no Discord token required), `set online|invisible` (writes override + applies once), `clear` (removes override + applies schedule). Override expires at the next schedule flip; stale file removed when expired on the next `run_once` / `override status`.
- **Manual override state:** JSON under `$XDG_STATE_HOME/sdss/override.json` or `~/.local/state/sdss/override.json` (atomic write).
- **`steam-discord-status-scheduler doctor`**: environment and desktop diagnostics (masked token, `xdg-mime` Steam handler, user systemd timers, **PySide6** import check for tray); `--json` for NDJSON lines on stdout.
- **Vacation / away ranges:** optional **`SDSS_VACATION`** (or legacy `vacationRanges` / `vacation`) — comma-separated inclusive `YYYY-MM-DD:YYYY-MM-DD` ranges; those local dates skip work-window invisibility like a public holiday. **`validate-config`** and **`override status`** show configured ranges.
- **Structured logging:** `SDSS_LOG_JSON=1` for JSON lines on stderr; default `[sdss]` timestamped prefix via `logutil`.
- **Steam / `xdg-open` errors:** actionable stderr line and anchor [docs/install.md#steam-display](docs/install.md#steam-display).
- **Optional `.desktop` files** under `packaging/linux/` for doctor / validate-config.
- **GitHub Actions CI** (ruff, mypy, pytest, build) on Python 3.11 and 3.12.

### Changed

- **`override set` / `override clear`:** exit code **1** when `apply_presence` fails (Discord/Steam), matching the default scheduler CLI; return value includes a `RunOnceResult` snapshot for tray consumers.
- **Tray icon asset:** default `tray.svg` is a neutral rounded-square hub instead of overlapping Discord/Steam circles.
- **Tray:** replaced **pystray** with **PySide6 (Qt 6)** for a real **context menu** and StatusNotifier-style integration (Wayland-friendly on KDE Plasma). **`[tray]`** optional deps are now **PySide6** only (removed `pystray` / Pillow from that extra). **SIGINT** is registered on the **main thread** before the Qt event loop (fixes `signal only works in main thread` when testing backends).
- **Scheduler run:** `run_once` (default CLI with no subcommand) uses `effective_work_mode`, so a valid `override.json` forces online/invisible until its `expires_at`; expired overrides are removed automatically.
- **Backend apply:** Discord and Steam are implemented in separate modules (`steam_discord_status_scheduler.backends.discord_apply`, `…backends.steam_apply`). Orchestration uses `apply_presence` with `ApplyTargets`; each enabled backend is updated **independently**—if the Discord HTTP call fails, Steam is still updated via `xdg-open` (previously Steam was skipped when Discord failed).
- **Layout:** systemd helper scripts live under `deploy/linux/systemd/` (replacing `scripts/`).
- **Documentation:** slimmer `README.md`; deeper Linux install and troubleshooting in `docs/install.md`.
- **systemd:** removed **headless** mode (oneshot + repeat/boundary timers). `install_user_units.py` only installs **`steam-discord-status-scheduler-tray.service`** and removes legacy timer/oneshot units. Dropped `.env` keys `useLogonTrigger`, `intervalMinutes`, `useCalendarTriggers`, `startWhenAvailable` (installer-only before).
- **Environment variables:** preferred `SDSS_*` names with legacy fallbacks:
  - `SDSS_DISCORD_TOKEN` or `discordAPI`
  - `SDSS_ENV_FILE` or `STEAM_DISCORD_SCHEDULER_ENV_FILE`
  - `SDSS_VERBOSE` or `STATUS_SCHEDULER_VERBOSE`
  - `SDSS_DISCORD_DEBUG` or `DISCORD_DEBUG`
- **Versioning:** package version is derived from Git tags via `hatch-vcs` (e.g. tag `v0.2.0`).
- **Examples:** added `examples/sample.env` (redacted template).

### Removed

- **Vacation ranges in env keys:** `SDSS_VACATION`, `vacationRanges`, and `vacation` are no longer read by the config loader. Vacations are managed exclusively via `vacations.json` (Settings dialog).
- **`[tray]` optional dependency group:** **pystray** and **Pillow** (tray-only); tray installs use **PySide6** only.
