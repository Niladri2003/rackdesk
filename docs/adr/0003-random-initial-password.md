# ADR 0003: Random initial password, never a default one

**Status:** accepted (M0)

## Context
A default like `administrator/administrator` is the easiest first run. But this program controls the server's files, bots try default credentials on any open port within minutes, and the UK (PSTI, 2024) and California (SB-327) ban universal default passwords on connected products.

## Decision
On first start, generate a random 20-character password (~116 bits) from an alphabet with no look-alike characters. Print it once to the terminal, not through the logger. Store only an argon2id hash in `credentials.json` (0600). Set `must_change_password` so the UI forces a change on first sign-in (M1). Recovery is `webdeskd reset-password`, run as root on the server (M1).

## Consequences
The first run is just as easy (copy the password from the install output), but nothing is guessable. The same pattern as Jenkins' initial admin password.
