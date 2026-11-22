# Structured log `MESSAGE_ID` values (journald)

These appear as structured `message_id` fields on `tracing` events. When built with the **`journald`** feature and running under systemd (`JOURNAL_STREAM` or `INVOCATION_ID` set), they are useful for filtering in `journalctl`.

| MESSAGE_ID | Meaning |
|------------|---------|
| `sdss_apply_fail` | Discord/Steam apply failed |
| `sdss_migrate_ok` | Legacy config imported to `config.toml` |
| `sdss_secret_fallback` | Discord token written/read via file fallback (keyring unavailable) |
| `sdss_discord_retry` | Discord HTTP retry (429 rate limit or 5xx backoff) |

Reserved targets: `sdss::apply`, `sdss::dbus`.
