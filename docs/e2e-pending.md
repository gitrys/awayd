# Pending E2E checks

Flows to cover in the next requested E2E run. Remove entries once they have been run.

## 2026-11-22 – Fix plan 1

- **Discord start re-apply (#76):** start Discord while `sdssd` runs → log shows one
  `Discord.exe started` (not six), the planned status is re-applied. Open question: does Discord
  overwrite the status again a few seconds after login (would need a delay)?
- **Unreadable config (#33):** break `config.toml` (syntax error, `schema_version = 2`), open
  `sdss-settings` with daemon online and offline → notice shown, edits are not saved, the file is
  unchanged; "Reload" after fixing the file clears the notice; "Back up and start over" writes a
  timestamped `.bak` and saving works again. First start without `config.toml` → wizard, no notice.
- **Holiday refresh offline (#62):** go offline, click "Refresh holiday data" → error mentions that
  existing data stays active, `data/holidays/<CC>.json` still exists; online refresh replaces it.
- **DST (#32):** not reproducible manually; covered by unit tests (chrono-tz) and `tests/dst_local.rs`.
