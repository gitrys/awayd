# Pending E2E checks

Flows to cover in the next requested E2E run. Remove entries once they have been run.

## 2026-11-22 – Fix plan 1

- **Discord start re-apply (#76), open question:** with a Discord token stored, does Discord
  overwrite the status again a few seconds after login (would need a delay)? (One
  `Discord.exe started` and one re-apply per Discord start: verified 2026-11-22.)
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

## 2026-11-22 – Fix plan 3 (autostart toggle, tray error state, build script)

- **Autostart toggle (GitHub #1):** Settings → Scheduler → "Start awayd at login": tick → value
  `awayd` in `HKCU\…\Run` with a quoted path; untick → removed, message "Autostart disabled".
  Point the value at another path by hand → settings shows "starts another file" with
  "Use this awayd.exe" / "Remove entry". Log off/on → awayd starts (also from a path with spaces).
- **Tray error state:** break `config.toml` while awayd runs and trigger a tick (Apply now) →
  amber error icon, tooltip "awayd — error: …", menu "Error: status not applied (see Logs)".
  Without a Discord token the normal mode icon stays, tooltip adds "· warning: …".

## 2026-11-22 – Fix plan 4 (validation, work-hours table, holiday coverage)

- **Work-hours table:** Settings → "Use same hours for all active days", change Start to 08:00,
  then tick "Use custom table" (or untick "Use same hours…") → the table and `config.toml` keep
  08:00 for all active days.
- **Invalid vacation:** with the daemon stopped, try to save a vacation whose end is before its
  start → "Not saved: vacation …: start after end", `config.toml` unchanged.
- **Holiday coverage:** a holiday file without the current/next year → status/tooltip warning
  "does not cover <year>", status still applied.

## 2026-11-22 – Backend abstraction (GitHub #3) and service switches (legacy issue 42)

- **Existing config:** start the new build with the 0.1.0 `config.toml` (no `[backends]`) →
  Steam and Discord are both on (Settings → Scheduler → Services), behaviour unchanged.
- **New install:** empty portable folder → wizard shows Steam on, Discord off, no token field;
  after "Finish" `config.toml` has `[backends.discord] enabled = false`; status has no
  "Discord token missing" warning, log has no Discord request.
- **Switch Discord on:** tick Discord → risk note and token field appear; enter the token →
  "Discord token saved", Credential Manager has `awayd` / user `discord`; status applied to
  Discord. Untick → Discord is no longer contacted (log), token stays stored.
- **Switch Steam off:** untick Steam, start Steam → log shows `steam.exe started` but no
  `presence sent` for Steam.
- **All off:** untick both → hint "No service is switched on"; status still ok, nothing sent.
- **Status per service:** `awayctl doctor --pretty` → `services: discord …, steam …` and one
  line per service under `last_apply` (sent / skipped (…) / failed (…)); tray "Last apply:
  failed — <service>: …" after an invalid token.
- **Typo in config:** `[backends.discrod]` → settings/daemon reject the config with
  "unknown service [backends.discrod] (known: discord, steam)".

## 2026-11-22 – Fix plan C1 (legacy issues 34, 78, 23, 64)

- **Network mode kept:** set `[mode] kind = "network"` by hand, open settings → the disabled
  checkbox is ticked with the hint; change a vacation and save → `kind` is still `"network"`.
- **Time field:** change an end time from 16:24 to 16:30 by typing → no save/apply while typing
  (log has no `presence sent` for "16:03"); after Enter the value is saved once.
- **Config save:** change a vacation far in the future → no `presence sent` in the log (target
  unchanged); switch a service on → it is applied.
- **Token field:** type a token slowly with pauses → nothing saved until Enter / leaving the
  field ("Press Enter or leave the field to save"), then one "Discord token saved".
- **CLI:** `awayctl doctor --json | ConvertFrom-Json` works; `$LASTEXITCODE` is 1 while the
  status reports `ok: no` and 0 when ok; `awayctl override set foo` → exit code 1.

## 2026-11-22 – Fix plan C2 (legacy issues 35, 9, 65)

- **Region fallback:** set `subdiv = "BWX"` in `config.toml` → status warning "region "BWX" not
  found … only nationwide public holidays apply"; 25 Dec still counts as a holiday (doctor
  next_wake / tray). Same with `subdiv = ""` ("no region set for DE").
- **Holiday downloads:** Settings → "Refresh holiday data" and the country list still work;
  while a refresh runs, the tray and `awayctl doctor` stay responsive.
- **Log level at runtime:** set `[advanced] log_min_level = "warn"` and save in the settings (or
  Reload) → log line "log capture level applied", no more INFO lines afterwards; back to
  "debug". (Timestamps with offset and the short time in the log view: verified 2026-11-22.)

## 2026-11-22 – Plan T (tray without polling)

- **Tray reacts without polling:** status change (override from the tray, or a schedule change)
  → icon and tooltip update at once; tray menu opens and its actions work; while the menu is
  open a status change is shown at the latest when the menu closes.

## 2026-11-22 – Plan U1 (lifecycle, IPC)

- **Wake-up / time change:** sleep and wake the PC (also by wake timer without input) → log
  `awayd::power resume (automatic)` (and `resume (user)` after input), one re-apply. Change the
  time zone in Windows settings → `system time changed` (if Windows sends it) or a re-apply
  within 15 minutes at the latest.
- **Second user:** with fast user switching, start awayd in a second Windows account → both run
  with their own tray; settings of each account talk to their own daemon.
- **Broken state.json:** stop awayd, write `{` into `state\state.json`, start → log warning
  "state.json unreadable … moved aside", `state.json.<time>.bak` exists, status works.
