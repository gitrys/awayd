# Security policy

## Supported versions

Only the latest release receives fixes.

## Reporting a vulnerability

Please report security problems **privately** via GitHub:
[Report a vulnerability](https://github.com/gitrys/awayd/security/advisories/new)
(Security tab → "Report a vulnerability"). Do not open a public issue for them.

Include the awayd version (`awayctl doctor` shows it), what you did and what happened. You will
get an answer as soon as possible; this is a spare-time project, so please allow some days.

## What to know about awayd's security

**Discord token**

- awayd changes your Discord status with your **Discord user token**. Anyone who has this token
  can act as your Discord account. Treat it like a password.
  [docs/discord-token.md](docs/discord-token.md) shows how to get it and how to end its session.
- The token is stored in the **Windows Credential Manager** (service `awayd`, user `discord`).
- Only if storing it there fails, awayd falls back to a **plain-text file**
  `state\credentials\discord.secret` next to the executables (portable mode) or under
  `%APPDATA%\awayd\state\credentials\`. A warning is written to the log in that case.
- With Discord switched off (the default for new installations) or without a token, awayd never
  contacts Discord and does not read the stored token.

**Local control interface**

- The settings window, the tray and `awayctl` talk to the daemon over the named pipe
  `\\.\pipe\awayd-<session id>` (one per Windows session). It uses the default Windows
  security for named pipes (no custom access list). Programs running **under your Windows
  account** can therefore connect and change the configuration, the override and the stored
  Discord token. Requests are limited to 1 MiB per line; malformed requests are answered with
  an error.

**Network access**

awayd only contacts:

- `https://discord.com/api/...` – to set your status (only when Discord is switched on and a
  token is stored),
- `https://date.nager.at` – to download public holidays for the configured country.

It sends no telemetry and no other data.

**Downloads**

Releases contain `SHA256SUMS.txt` and a minisign signature made with the key in
[`minisign.pub`](minisign.pub). See "Verify downloads" in the [README](README.md).
