# `Scheduler1.Status()` JSON shape

Daemon exposes a JSON string via D-Bus (`org.steamdiscord.Scheduler1.status`).

| Field | Type | Meaning |
|-------|------|--------|
| `ok` | bool | Last evaluated tick completed without I/O errors (apply may still fail per-backend). |
| `schema` | number | Snapshot format version (currently `1`). |
| `work_mode` | bool | Effective “invisible work mode” (`true` = Discord/Steam should be work/invisible side). |
| `source` | string | `"schedule"`, `"manual_override"`, or `"network_stub"`. |
| `mode_kind` | string | Copy of `[mode].kind` from `config.toml` (`schedule` / `network`). |
| `next_wake` | string / null | ISO 8601 local-ish timestamp of next scheduled wake-up. |
| `last_apply` | object / null | Last apply attempt from `state.json`. |
| `errors` | array of string | Non-fatal issues (e.g. missing token). |

### `last_apply` object

| Field | Type | Meaning |
|-------|------|--------|
| `ok` | bool | Both Discord and Steam apply succeeded. |
| `work_mode` | bool | Mode that was applied (`true` = invisible). |
| `source` | string | `"schedule"`, `"manual_override"`, or `"network"`. |
| `at` | string | Timestamp `%Y-%m-%dT%H:%M:%S` (local). |
| `discord_error` | string / null | Discord failure message when `ok` is false. |
| `steam_error` | string / null | Steam failure message when `ok` is false. |

Daemon logs (NDJSON) live under `{state}/logs/sdss.log` (rotated). See Settings **Logs** tab or `sdss logs`.

When you run `sdss doctor --json` and the daemon responds on the bus, `doctor` may merge a **`holiday_warnings`** field (array of strings) into the JSON object parsed from `Status()` when `config.toml` loads successfully. The daemon itself does not set `holiday_warnings`; see `holiday_coverage_warnings` in `src/holidays/data.rs`.

ISO timestamps use `%Y-%m-%dT%H:%M:%S` in local wall-clock (no explicit offset).
