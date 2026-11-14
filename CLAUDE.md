# am-remote — Apple Music remote control (monorepo)

Control the Apple Music app on a Windows laptop from an Android phone over local Wi-Fi.
Audio always plays on the laptop; the phone is only a remote.

## Projects
| Folder | What | Stack |
|---|---|---|
| `desktop-agent/` | Background `.exe` on the laptop. Talks to Apple Music + Windows, exposes an HTTP/WebSocket API on the LAN. | Rust (tokio, axum, windows-rs) |
| `android-controller/` | Android remote app. Discovers the agent, sends commands, shows now playing. | Flutter |

Each project has its own `CLAUDE.md` with stack, structure and conventions. This file holds what both share:
the roadmap and the API contract. **The API changes only from the root window (`/contract-change`): this file first,
then `contract/samples.md` and `contract/CHANGELOG.md`. Each project then applies the change with `/sync-contract`.**

## Workspace (one VS Code window per folder; this file loads in all of them)
- **Root window:** maintenance only. It edits the contract, docs and every `.claude/`, never project source code.
- **Project windows:** edit only their own folder; `../CLAUDE.md` and `../contract/` are read-only there.
- Step-by-step tasks live in `<project>/.claude/tasks/*.md`. Window roles and hand-offs are described in README.md.

## Roadmap (MVP phases)
- **MVP 0 — Foundation:** agent server, tray icon, pairing token, mDNS discovery; app connect screen.
- **MVP 1 — Playback controls:** play, pause, toggle, next, previous, volume, mute.
- **MVP 2 — Now playing:** title/artist/album/artwork, position, live push over WebSocket, media notification on phone.
  - Spike: read current playlist/queue via UI Automation (SMTC only exposes the current track).
- **MVP 3 — Playlists & search:** list saved playlists, play a playlist, search the catalog, play a track.
  - Spike first: does opening a `music.apple.com` link play it in the desktop app? Decides link-based vs UI-Automation approach.

Build MVP 0 → 1 → 2 first (stable Windows APIs). Run the spikes before committing to MVP 3.

## Known platform limits (don't try to "fix" these the hard way)
- SMTC (Windows media session API) gives play/pause/skip/seek + metadata of the current track only. No queue, no playlists, no volume.
- Volume comes from Windows Core Audio per-app session volume, not SMTC.
- Playlists/queue need UI Automation of the Apple Music window — fragile, breaks on app updates. Isolate it behind a trait/module.
- Out of scope: hooking/injecting into Apple Music, touching DRM, imitating Apple's private server API.

## API contract (v1)
Base: `http://<laptop-ip>:8765/api/v1` · JSON everywhere · times in ms · volume 0–100.

### Discovery
mDNS service `_amremote._tcp.local.`, port 8765, TXT: `version=1`, `name=<hostname>`.

### Auth / pairing
- Agent shows a 6-digit pairing code in its tray menu.
- `POST /pair` `{ "code": "123456", "device_name": "Pixel 8" }` → `{ "token": "<random 32+ bytes, base64url>" }`
- All other endpoints require header `Authorization: Bearer <token>`. WebSocket uses `?token=<token>`.
- Tokens persist on the agent (config file) until revoked from the tray menu.

### Endpoints
| Method | Path | Body / Query | Response | MVP |
|---|---|---|---|---|
| GET | `/status` | – | `{ agent_version, apple_music_running }` | 0 |
| POST | `/player/play` · `/pause` · `/toggle` · `/next` · `/previous` | – | `204` | 1 |
| GET | `/volume` | – | `{ level, muted }` | 1 |
| POST | `/volume` | `{ level?, muted? }` | `{ level, muted }` | 1 |
| GET | `/player/state` | – | `PlayerState` | 2 |
| GET | `/player/artwork` | – | `image/jpeg` (`404` if none) | 2 |
| POST | `/player/seek` | `{ position_ms }` | `204` | 2 |
| GET | `/playlists` | – | `[ { id, name } ]` | 3 |
| POST | `/play/playlist` | `{ id }` | `204` | 3 |
| GET | `/search` | `?q=&limit=25` | `[ SearchResult ]` | 3 |
| POST | `/play/track` | `{ url }` | `204` | 3 |

```jsonc
// PlayerState
{ "status": "playing|paused|stopped|unknown",
  "title": "…", "artist": "…", "album": "…",
  "duration_ms": 215000, "position_ms": 42000,
  "artwork_rev": 7,            // increments when artwork changes → re-fetch /player/artwork
  "playlist_name": null }      // filled only if the MVP 2 UIA spike succeeds

// SearchResult
{ "id": "…", "title": "…", "artist": "…", "album": "…",
  "artwork_url": "https://…", "url": "https://music.apple.com/…" }
```

### WebSocket `GET /ws?token=…`
Server → client messages: `{ "type": "<event>", "data": { … } }`
- `state` → `PlayerState` (sent on connect and on every change)
- `volume` → `{ level, muted }`
- `apple_music` → `{ running: bool }`
Client → server: `{ "type": "ping" }` every 15 s; server replies `{ "type": "pong" }`.

### Errors
Non-2xx bodies: `{ "error": { "code": "…", "message": "…" } }`
Codes: `UNAUTHORIZED` (401), `BAD_PAIRING_CODE` (403), `APPLE_MUSIC_NOT_RUNNING` (409), `NOT_SUPPORTED` (501), `INTERNAL` (500).
