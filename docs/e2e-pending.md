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

## 2026-11-22 – Plan N (network mode with ping)

- **Work PC at home:** `[mode] kind = "network"`, `work_host = "<IP of the work PC>"`,
  `probe = "ping"` → work PC on: status invisible (source `network`) within about a minute;
  switch it off: online after about two minutes (two missed pings); no change on a single
  lost ping.
- **Override in network mode:** tray → override "until next change" → `state.json` expiry is
  now + `network_override_ttl_hours` (default 8 h).

## 2026-11-22 – Plan V (settings: background requests, screen reader)

- **No freeze:** throttle or cut the network, Settings → "View public holidays…" → "Refresh
  holiday data" → button shows "Refreshing holiday data…" with a spinner, the window stays
  movable and responsive; the result (success or error) appears when the download ends. First
  start of the settings after a daemon start: "Country (loading list…)" until the list is
  there.
- **Screen reader:** start Narrator (Ctrl+Win+Enter), tab through the settings → buttons,
  fields and checkboxes are read with their names; the icon buttons read "Pick date range",
  "Pick time", "Remove vacation", "Previous/Next month", "Previous/Next year".

## 2026-11-23 – Plan UI-1 (redesigned settings window, theme choice)

Older entries above name the old places: "Scheduler" is now split into **Mode** (work hours),
**Days off** (region, holidays, vacations) and **Services**; autostart, zoom and the wizard
button are under **General**.

- **Layout:** open the settings → sidebar with Mode, Days off, Services, General, Logs; each
  page shows its cards; nothing from the old tabs is missing (holiday list flyout, vacations
  with date picker, token field, autostart repair texts, zoom slider, log pop-out).
- **Theme:** General → Display → "Light" and "Dark" switch at once; "Use Windows setting"
  follows Windows (Settings → Personalization → Colors → app mode) while the window is open;
  the choice is kept after closing and reopening. Read all pages in both themes (text, grey
  descriptions, warnings, the Discord risk note, the status bar).
- **Switches:** Services (Steam/Discord), "Start awayd at login" and "Same hours for all work
  days" toggle with the mouse and with Tab + Space; saving and autostart behave as before;
  "Same hours" off makes the day rows editable, on makes the Start/End row editable.
- **Status bar:** a save shows "✔ saved" bottom left and fades out; an error (e.g. a vacation
  with reversed dates) stays red; right side shows the daemon state and the config path.
- **Banners:** different daemon version or an unreadable `config.toml` → notice at the top of
  the page; "Reload" and "Back up and start over" still work.
- **Window size:** a `settings_ui.json` from 0.2.0 (580×480) opens at least about 640×440 and
  is saved with at least 720×480 when the window closes; without the file the window opens at
  960×680; it cannot be made smaller than about 640×440.
- **Narrator:** sidebar entries are read with their name and "selected"; switches with their
  name and on/off.
- **Font:** the text uses Segoe UI like the Windows settings; if it looks like egui's own
  font instead, the log has "Segoe UI not loaded" with the reason.

## 2026-11-23 – Plan UI-1b (settings: fixes and mockup look)

- **Window size and position:** set zoom to 115 %, make the window larger and move it, close
  and reopen → same size and place (before: about 15 % smaller and further right each time).
- **Scroll bar:** make the window short → a scroll bar is visible on the right without
  hovering; all buttons reachable.
- **Rows:** General → Theme and Startup: the description wraps, nothing overlaps.
- **Sidebar:** icons next to the pages; the "awayd is running" box sits at the bottom in its
  own height.
- **Days off:** holidays of the year listed on the page, ◀/▶ changes the year; "Refresh
  holiday data" works; vacations line up in columns with "N days"; adding one via the last
  row (name, then calendar) still works; country/region open the search list.
- **Logs:** the page opens at the newest line and follows new lines; scrolling up stops
  following; "Verbose" off hides DEBUG lines (e.g. hyper_util), "Info" off hides INFO lines;
  "Pop out" opens the separate window with the same table.

## 2026-11-23 – Plan UI-2 (network mode in the settings)

- **Switch mode:** Mode → "Network presence" → host field empty → warning, nothing saved
  (`config.toml` still `kind = "schedule"`); enter your work PC's IP, leave the field →
  "saved", `config.toml` has `kind = "network"` and `work_host`; `awayctl doctor` shows
  `mode_kind: network`. Back to "Work hours" → saved, the week table is shown again.
- **Test now:** with the work PC on → "Answered to ping in N ms"; with an unknown name →
  "Not checked: host name not found"; with the PC off → "No answer to ping within 1 s"; the
  status in the tray does not change by testing.
- **Check by:** "TCP port" enables the port field; change port, interval and override hours
  (dragging or typing + Enter) → saved; values outside the limits cannot be entered.
- **Wizard:** General → "Run setup wizard again…" → step 3 "Mode"; "Network presence" → next
  step is the device (step 4 of 4), "Finish" without a host shows an error; "Work hours" →
  region and work hours as before (5 steps).
- **Day at a glance:** each work day shows its window on a 0–24 h bar (hover shows the times);
  with "Same hours" on, the bars follow the Start/End row.

## 2026-11-23 – Plan UI-3 (Overview page)

- **Status:** open the settings → Overview first; current status ("Invisible"/"Online"), the
  reason, the next change with "in …", the mode; matches the tray menu and `awayctl doctor`.
- **Override:** pick "Online", "1 hour", "Set override" → status bar "Override set", within
  2 s the card shows "Override active: Online until …", the tray agrees; "Clear override" →
  back to the schedule. "Until the next change" in network mode lasts the override hours.
- **Apply now:** click → status bar message; the log shows a new apply; "Last apply" time
  updates within 2 s.
- **Last apply:** Steam "Sent", Discord "Skipped · no discord token configured" (or "Off"
  when switched off); the same rows appear on the Services page with "Applied again when it
  starts: steam.exe / Discord.exe".
- **Warnings:** with Discord on and no token the warning from the daemon is shown below the
  status card.
- **Daemon offline:** quit awayd from the tray → Overview says awayd is not running.
