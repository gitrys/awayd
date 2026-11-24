# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Discord and holiday download errors now name the underlying cause (e.g. DNS lookup, refused
  connection, TLS) in the log and in the error shown to the user. Before, they only said
  "error sending request for url (…)", so a failure after a restart could not be told apart.

## [0.3.1] - 2026-11-23

### Upgrading from 0.3.0

- Replace all three programs (`awayd.exe`, `awayd-settings.exe`, `awayctl.exe`) together.
  `config.toml`, the Discord token and the saved state are kept.
- **Log files to keep** now counts the current `awayd.log` too: with the default 5, awayd keeps
  `awayd.log` and `awayd.log.1` to `.4`; an `awayd.log.5` is deleted at the next rotation.

### Fixed

- Settings: the country picker under **Days off** offered only Germany. At start the settings
  ask awayd for the country list and for the stored credentials at the same moment; the second
  connection was refused with "All pipe instances are busy". Clients (settings and `awayctl`)
  now retry a busy pipe for up to 2 s, and the settings ask for the country list again every
  30 s while the picker is shown without it. The same race could make **Services** show a stored
  Discord token as missing.
- awayd logs a warning when it cannot download the country list instead of silently answering
  with an empty list.
- Log rotation kept one file more than **Log files to keep** says: `advanced.log_max_files` now
  counts the current `awayd.log` too (default 5: `awayd.log` and `awayd.log.1` to `.4`, about
  25 MB). Rotated files beyond the limit, e.g. `awayd.log.5` left by 0.3.0, are deleted at the
  next rotation; with 1 the log file starts over.
- A failed log rotation (e.g. another program holds `awayd.log` open without delete sharing)
  dropped the log line that triggered it and was retried on every line; awayd now keeps
  writing to the current file and tries again later.

## [0.3.0] - 2026-11-23

### Upgrading from 0.2.0

- Replace all three programs (`awayd.exe`, `awayd-settings.exe`, `awayctl.exe`) together:
  the settings use new named-pipe methods (`probe_host`, `resource_use`) that an awayd 0.2.0
  does not know.
- `config.toml`, the Discord token and the saved state are kept; the new setting
  `advanced.process_watch_seconds` defaults to 5 s (0.2.0 checked every 2 s).
- Windows may show the old program icon from its icon cache for a while; re-pin a shortcut
  or restart Explorer to see the new one.

### Added

- Settings: theme choice under **General → Display** ("Use Windows setting", "Light",
  "Dark"), stored in `settings_ui.json`. Until now the window followed the Windows app theme
  without a choice.
- Network mode in the settings: **Mode** offers "Work hours" or "Network presence"; for the
  latter the host, the check (ping or TCP port), the check interval and the override duration
  are set there, with the same limits as `awayctl validate`. **Test now** asks awayd to check
  the host once (new named-pipe method `probe_host`; it does not change the status). Until a
  host is entered the choice is shown but not saved.
- The setup wizard asks for the mode and, for network presence, for the device instead of
  region and work hours.
- Work hours table: a "Day at a glance" bar per day.
- Settings page **Overview** (opens first): current status with its reason, the next change,
  the mode, the last apply per service (sent, skipped or failed with the reason), the holiday
  data and the daemon's warnings; an override (invisible or online, until the next change or
  for 30 min to 8 h) and "Apply now" without the tray. The status is refreshed every 2 s while
  the page is shown.
- Services page: the last apply and the program whose start makes awayd apply again, per
  service.
- Settings: **Remove token** deletes a stored Discord token from the Credential Manager.
- General page: **Advanced** (log level, log file size and count, wait before re-applying;
  until now only in `config.toml`), **Configuration** (path, "Open folder", "Reload from
  file", setup wizard) and **About** (versions, license, project page).
- "Start awayd" in the sidebar when awayd is not running, and "Restart awayd" in the notice
  shown when the running awayd.exe is another version; "Open log folder" on the Logs page.
- New setting `advanced.process_watch_seconds` (2–60 s, 0 = off; default 5), in the settings
  under **General → Advanced → Watch for program starts**.
- **General → Resource use** shows awayd's memory and CPU time since start (asked for only
  while the page is open; new named-pipe method `resource_use`).
- `awayd-settings --page <page>` opens a page directly; a second start brings the open
  settings window to the front instead of doing nothing.
- Guide with screenshots on how to get the Discord token and store it in awayd
  ([docs/discord-token.md](docs/discord-token.md)), linked from the README, the install guide
  and SECURITY.md.

### Changed

- The settings window is redesigned in the style of the Windows 11 settings app: a sidebar
  with the pages **Mode**, **Days off**, **Services**, **General** and **Logs** replaces the
  three tabs, settings are grouped in cards, light and dark colours are checked for contrast,
  and Segoe UI is used when Windows has it. Services, autostart and "Same hours for all work
  days" are switches (the last one replaces the two opposite checkboxes "Use same hours…" and
  "Use custom table"). The result of the last action, the daemon state and the config file
  are shown in a status bar at the bottom. The default window size is 960×680.
- Settings pages: the sidebar has icons; scroll bars are always visible; long descriptions
  wrap instead of running under the controls; the public holidays of the selected year are
  listed on the **Days off** page (with year navigation and "Refresh holiday data") instead of
  a flyout; vacations are a table with the number of days; country and region look like
  drop-down fields; each service has a short description.
- Logs page: a table with time, level, source and message; separate filters for Verbose
  (DEBUG/TRACE, off by default), Info, Warnings and Errors; "Pop out" next to the title.
- New app icon (light-blue tile with a half-filled circle, the mark of the settings sidebar)
  for all three programs and the settings window's title bar and taskbar button.
- Settings: the window cannot be made smaller than 896×640; the General page uses two
  columns only when there is room; rows put wide controls (theme buttons, zoom slider) under
  their text.
- Less load while idle: awayd notices program starts by comparing the list of process IDs
  and reads the names of new processes only (a full process snapshot with all names took
  about 7.6 ms per check, the list about 0.03 ms). The check runs every 5 s instead of 2 s,
  only for programs of switched-on services, and rests completely when there are none. The
  background runtime uses 2 worker threads instead of one per CPU core. Measured idle over
  60 s: 297 ms CPU per minute and 37 threads before, below the measurement resolution and 9
  threads after.
- New tray icon: a one-colour glyph (white on a dark, black on a light taskbar, following the
  Windows setting at once) whose shape shows the state: empty ring = online, half-filled
  circle = invisible, dashed ring = status unknown; a blue dot marks an override, a yellow
  triangle a warning. Drawn at the real icon size for the screen scaling (awayd is now DPI
  aware) and cached, so status changes cost no drawing. Replaces the 2×2 colour grid that
  told the states apart by colour only.
- New tray menu: the current status with "until …" and its reason at the top, a warning
  line only when there is a problem (click opens the matching settings page), "Invisible ▸"
  and "Online ▸" with the same durations as the Overview page, "Clear override" only while
  one is active, "Apply now", "Open settings…", "Quit". The version line and "More…" are
  gone. Menus are dark in Windows dark mode. Left click on the icon opens the settings, right
  click the menu. The tooltip says the state in plain words ("awayd · Invisible until 17:00
  (work hours)"), shortened to the 64 characters the taskbar shows.

### Fixed

- Settings: vacation names were cut to a few letters; texts in multi-column layouts no longer
  have stretched word gaps; the config path is shown in normal text.
- The settings window opened smaller by the zoom factor (e.g. 115 %) every time and moved
  right and down; size and position are now restored as they were left.
- Logs: "Follow new lines" keeps the view at the newest line (it only jumped there once).

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

[Unreleased]: https://github.com/gitrys/awayd/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/gitrys/awayd/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/gitrys/awayd/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/gitrys/awayd/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/gitrys/awayd/releases/tag/v0.1.0
