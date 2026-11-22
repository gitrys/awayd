# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

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
