# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/gitrys/awayd/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/gitrys/awayd/releases/tag/v0.1.0
