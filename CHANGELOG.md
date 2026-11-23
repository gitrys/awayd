# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Settings: theme choice under **General → Display** ("Use Windows setting", "Light",
  "Dark"), stored in `settings_ui.json`. Until now the window followed the Windows app theme
  without a choice.

### Changed

- The settings window is redesigned in the style of the Windows 11 settings app: a sidebar
  with the pages **Mode**, **Days off**, **Services**, **General** and **Logs** replaces the
  three tabs, settings are grouped in cards, light and dark colours are checked for contrast,
  and Segoe UI is used when Windows has it. Services, autostart and "Same hours for all work
  days" are switches (the last one replaces the two opposite checkboxes "Use same hours…" and
  "Use custom table"). The result of the last action, the daemon state and the config file
  are shown in a status bar at the bottom. The default window size is 960×680.

## [0.2.0] - 2026-11-23

### Upgrading from 0.1.0

- Replace all three programs (`awayd.exe`, `awayd-settings.exe`, `awayctl.exe`) together: the
  named pipe and its methods changed, programs of 0.1.0 cannot talk to a 0.2.0 daemon.
- Enter the Discord token again in the settings; the token saved by 0.1.0 is not taken over.
- Existing `config.toml` files keep working; both services stay on.

### Added

- Steam and Discord can be switched on and off individually (Settings → Scheduler → Services,
  wizard, or `[backends.<id>] enabled` in `config.toml`). A switched-off service is never
  contacted and its token is not read. `awayctl doctor` shows which services are on.
- New installations start with **Discord off** and Steam on; the settings show the Terms of
  Service risk next to the Discord switch. Configurations from 0.1.0 (no `[backends]` section)
  keep both services on.
- `config.toml` with an unknown service id (e.g. a typo in `[backends.…]`) is rejected with the
  list of known services.
- Network mode: new `mode.probe = "ping"` (ICMP echo without administrator rights) besides
  `tcp`, e.g. to detect a work laptop in the home network whose firewall blocks all ports;
  `work_host` is checked every `mode.probe_interval_seconds` (default 60) instead of only at
  schedule changes; two failed checks in a row are needed for "absent"; host names are
  resolved without blocking the daemon; an override "until the next change" lasts
  `mode.network_override_ttl_hours` (the setting existed but was ignored). The config check
  requires `work_host` in network mode and plausible values for both settings.
- Screen reader support (Narrator, NVDA) in the settings window via AccessKit; icon-only
  buttons (calendar, clock, remove, previous/next) have names.

### Fixed

- Saving in the settings no longer switches a network mode set in `config.toml` back to the
  schedule; the settings show when network mode is active.
- Time fields take the value on Enter, when leaving the field or with "OK" in the clock popup,
  not while typing ("16:3" no longer saves and applies 16:03 on the way to 16:30).
- Credentials (Discord token) are saved on Enter or when leaving the field, no longer 0.8 s
  after each key press (partial tokens were stored and sent).
- Saving the configuration only re-applies services whose status changes (or that were just
  switched on) instead of re-sending the status to every service.
- `awayctl doctor --json` prints one valid JSON object (paths, daemon status, local config).
  `awayctl doctor` exits with 1 when the daemon is not running or reports problems, and
  `awayctl override set/clear` exit with 1 when the daemon rejects the request.
- An empty or unknown region (`schedule.subdiv`) no longer means "no public holidays at all":
  the nationwide holidays apply and the status shows a warning naming the region.
- Holiday downloads (background sync, "Refresh holiday data", country list) no longer block
  the daemon's async worker threads (debug builds panicked, so the background sync stopped).
- Logs: structured fields are redacted like the message (and values of `*_token` / `*_secret`
  fields are dropped), raw Discord tokens are redacted, timestamps carry the UTC offset
  (`…T17:59:21.311+01:00`), and a changed `advanced.log_min_level` takes effect on save /
  reload without restarting the daemon.
- Starts of Discord and Steam are detected again on systems where other programs use up the
  per-user WMI polling quota (e.g. Logitech G HUB with many running processes): awayd now
  compares process snapshots every 2 s instead of a WMI subscription, which failed with
  `0x8004106C` and logged a warning every 10 s. The `wmi` dependency is gone.
- A Discord rate limit no longer blocks awayd: a `Retry-After` longer than 5 s is not waited
  for inside the apply (overrides, "Apply now", resume and quit used to hang for that time,
  up to three times). Discord is held back until the given time while the other services
  keep working, the status shows "next attempt at …", and Discord is no longer retried on
  awayd's own 30–120 s backoff during the rate limit.
- Quitting is no longer lost when it arrives while a status change is being applied (the
  process could stay without tray icon and block a new start).
- Holiday download: a year Nager.Date does not deliver no longer discards all other years.
  Only this and next year are required; a missing other year is logged and kept from the
  previous data. Each year is retried once.
- The log records "awayd started" (version, PID, portable mode, config path) and "awayd
  stopping", so restarts and crashes are visible; a second instance logs that it exits.
- The settings log view no longer misses or garbles lines when the log rotates and the new
  file is already larger than the old read position.
- The tray no longer wakes up every 16 ms: it waits for window messages and is woken on status
  changes (fewer CPU wake-ups, relevant for laptop battery life).
- Build without compiler warnings (unused widgets and the unused `image` / `thiserror`
  dependencies removed); CI now treats warnings as errors in the MSRV check.
- The status is re-applied after any wake-up from standby (also automatic ones without user
  input) and after a change of system time or time zone; in addition awayd recomputes at least
  every 15 minutes, so a missed event is corrected within that time.
- One awayd per Windows session: with several logged-in users each gets their own (single
  instance per session, named pipe `\\.\pipe\awayd-<session id>`). Programs of different
  builds must be updated together.
- The named pipe answers malformed requests with an error instead of closing the connection
  and rejects requests over 1 MiB.
- An unreadable `state.json` or one with another schema version no longer makes every status
  update fail: it is moved aside as `state.json.<time>.bak` and a fresh state is used.
- Vacations get unique ids: after deleting one, a new vacation no longer reuses an id still in
  use (date pickers of two entries shared their state). Duplicate ids in existing configs are
  renumbered when the settings load them.
- `awayctl override --ttl 1h30` is rejected with a hint (`1h30m`) instead of meaning 1 h 30 s.
- The country list for the settings is downloaded without blocking other requests for it.
- The settings window no longer freezes while holiday data is refreshed or the country list
  is downloaded: both run in the background, with a spinner / "loading list…".

### Changed

- Steam and Discord are now two implementations of one backend interface; the scheduler, the
  status, the tray and `awayctl doctor` no longer contain service-specific code, so further
  services can be added without touching them.
- Status and `state.json` report the last apply per service (`last_apply.backends`: sent,
  skipped or failed with message) instead of `discord_error` / `steam_error`. Files and status
  replies written by 0.1.0 are still read.
- Credentials are stored per service: Windows Credential Manager user `discord` (was
  `discord-token`), fallback file `state\credentials\discord.secret` (was
  `state\discord_token.secret`). **A Discord token saved with 0.1.0 is not taken over: enter it
  again in the settings.**
- The named-pipe methods `has_token` / `set_token` / `clear_token` are replaced by
  `has_credential` / `set_credential` / `clear_credential` with a service id. Settings, tray and
  `awayctl` of 0.1.0 cannot set a token with a newer daemon; always update all three programs.

## [0.1.0] - 2026-11-22

First public release of awayd for Windows.

### Added

- Resident agent `awayd.exe` with a system tray icon: switches Steam and Discord between
  *invisible* during work hours and *online* outside of them.
- Schedule: work hours per weekday, public holidays by country and region (downloaded from
  Nager.Date and cached), vacation ranges.
- Manual override from the tray or `awayctl override`, for a fixed time or until the next
  scheduled change.
- Re-applies the status when Discord or Steam starts (WMI process events) and after resume
  from standby.
- Settings window `awayd-settings.exe`: schedule, region, vacations, Discord token (stored in
  the Windows Credential Manager), "Start awayd at login" toggle, live log view.
- Command-line client `awayctl.exe`: `doctor`, `logs`, `override`, `validate`, `holidays`,
  `install-autostart` / `uninstall-autostart`.
- Tray error state: an amber icon and an error tooltip when a status could not be applied;
  warnings (e.g. no Discord token) keep the normal icon.
- Version shown in the settings window, the tray menu and `awayctl doctor`.
- Portable release ZIP with third-party license notices, SHA-256 checksum and a minisign
  signature.

### Notes

- Discord is changed through Discord's web API with your user token, which is very likely
  against Discord's Terms of Service. See the warning in the README; without a token awayd
  only manages Steam.
- The executables are not code-signed yet; Windows SmartScreen may warn on first start.

[Unreleased]: https://github.com/gitrys/awayd/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/gitrys/awayd/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gitrys/awayd/releases/tag/v0.1.0
