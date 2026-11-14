---
paths:
  - "src/api/**"
  - "src/config.rs"
  - "src/tray.rs"
  - "src/main.rs"
---

# Security (pairing, tokens, LAN exposure)

- **Tokens:** 32 random bytes from the OS CSPRNG (`rand::rngs::OsRng` or `getrandom`), encoded base64url without padding. Compare them in constant time (`subtle::ConstantTimeEq`), or store and compare SHA-256 hashes.
- **Pairing code:** 6 digits from the CSPRNG, single use. Rotate it after a successful pair and show the new one in the tray. After 5 wrong attempts, refuse `/pair` for 60 s (still `403 BAD_PAIRING_CODE`).
- **Never log secrets:** no tokens, pairing codes, `Authorization` headers, or full request URIs (the WebSocket URL carries `?token=`). If you use tower-http `TraceLayer`, log the path only, not the query.
- **Config file:** `%APPDATA%\am-remote\config.json` holds tokens. Write it atomically (temp file, then rename). Never copy it into the repo, logs or test fixtures; tests use obviously fake tokens like the one in `../contract/samples.md`.
- **Revocation** from the tray takes effect immediately: drop the token and close that device's open WebSocket.
- **Exposure:** serve only the contract API. No CORS headers (the client is a native app), no file serving, no shell-outs built from request input, no UPnP or port forwarding. The listener binds to the LAN; the README tells users to allow it on **Private** networks only.
- **Out of scope** (see `../CLAUDE.md`): hooking or injecting into Apple Music, touching DRM, imitating Apple's private server API.
