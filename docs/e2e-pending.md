# Pending E2E checks

Flows to cover in the next requested E2E run. Remove entries once they have been run.

## 2026-11-22 – Fix plan 1

- **Discord start re-apply (#76):** start Discord while `awayd` runs → log shows one
  `Discord.exe started` (not six), the planned status is re-applied. Open question: does Discord
  overwrite the status again a few seconds after login (would need a delay)?
- **Unreadable config (#33):** break `config.toml` (syntax error, `schema_version = 2`), open
  `awayd-settings` with daemon online and offline → notice shown, edits are not saved, the file is
  unchanged; "Reload" after fixing the file clears the notice; "Back up and start over" writes a
  timestamped `.bak` and saving works again. First start without `config.toml` → wizard, no notice.
- **Holiday refresh offline (#62):** go offline, click "Refresh holiday data" → error mentions that
  existing data stays active, `data/holidays/<CC>.json` still exists; online refresh replaces it.
- **DST (#32):** not reproducible manually; covered by unit tests (chrono-tz) and `tests/dst_local.rs`.

## 2026-11-22 – Fix plan 2a

- **Override during a tick (#1):** set an override from the tray right while a status change is
  being applied (e.g. right after Discord starts) → the override stays active, `state.json` keeps
  the `override` entry.
- **First start without network (#20):** empty data folder (no `data/holidays/<CC>.json`), network
  off, start `awayd` → status is applied anyway, status/tray shows the "holidays are ignored"
  warning, a manual override works. Turn the network on → within about 5 min the log shows
  `cached holiday data from Nager.Date` and a new tick runs.

## 2026-11-22 – Fix plan 2b

- **Steam not running (#3):** quit Steam, trigger a status change ("Apply now" or override) →
  Steam is **not** started; log shows `presence skipped … steam is not running`. Start Steam →
  log shows `steam.exe started`, then `presence sent` for Steam.
- **Rejected / missing Discord token (#2):** set an invalid token → one `presence failed`
  (`permanent=true`), status shows "not retried automatically until the token is changed", no
  further Discord requests every 30–120 s, Steam is not re-triggered. Set a valid token → Discord
  is applied again. Remove the token → Discord `presence skipped`, no retries.
- **WMI query with two names:** after a reboot, `cargo test -p awayd-daemon --lib typed_event`
  (local WMI quota was exhausted on 2026-11-22, see commit 42c832a).

## 2026-11-22 – Fix plan 3 (autostart toggle, tray error state, build script)

- **Autostart toggle (GitHub #1):** Settings → Scheduler → "Start awayd at login": tick → value
  `awayd` in `HKCU\…\Run` with a quoted path; untick → removed, message "Autostart disabled".
  Point the value at another path by hand → settings shows "starts another file" with
  "Use this awayd.exe" / "Remove entry". Log off/on → awayd starts (also from a path with spaces).
- **Tray error state:** break `config.toml` while awayd runs and trigger a tick (Apply now) →
  amber error icon, tooltip "awayd — error: …", menu "Error: status not applied (see Logs)".
  Without a Discord token the normal mode icon stays, tooltip adds "· warning: …".
