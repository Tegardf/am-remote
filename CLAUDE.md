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

- **MVP 4 — Queue, shuffle/repeat & app search:** shuffle and repeat buttons; a scrollable Up Next queue under the player controls, tap a row to play it; search goes through the Apple Music app's own search and its results go back to the phone (replaces the iTunes Search API flow).
  - Spike done (`desktop-agent/docs/spikes/mvp4.md`); contract C-006 written from it. Next: `mvp-4.md` task for each project, then `/build-mvp 4`.

Build MVP 0 → 1 → 2 first (stable Windows APIs). Run the spikes before committing to MVP 3 and MVP 4.

## Known platform limits (don't try to "fix" these the hard way)
- SMTC (Windows media session API) gives play/pause/skip/seek + metadata of the current track only. No queue, no playlists, no volume.
- Volume comes from Windows Core Audio per-app session volume, not SMTC.
- Playlists/queue need UI Automation of the Apple Music window — fragile, breaks on app updates. Isolate it behind a trait/module.
- Out of scope: hooking/injecting into Apple Music, touching DRM, imitating Apple's private server API.

## API contract (v1)
Base: `http://<laptop-ip>:8765/api/v1` · JSON everywhere · times in ms · volume 0–100.

### Discovery
mDNS service `_amremote._tcp.local.`, port 8765, TXT: `version=1`, `name=<hostname>`, `ip=<ipv4>`.
The agent listens on IPv4 only, so a client uses `ip` as the host. If `ip` is missing it uses any IPv4 address the resolver gave it, and skips an agent that only resolved to IPv6. Clients list agents whatever their `version` and may warn when it is not `1`.
The agent re-publishes `ip` whenever its LAN IPv4 address changes (DHCP renewal, Wi-Fi switch). A client with a saved (paired) session that cannot reach the `ip` host falls back to an IPv4 address the resolver gave it, then re-runs discovery. When pairing from the discovery list the client may try only the `ip` host; the user can retry or enter the address by hand.

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
| GET | `/player/artwork` | – | image bytes, `image/*` (`404` if none) | 2 |
| POST | `/player/seek` | `{ position_ms }` | `204` | 2 |
| GET | `/playlists` | – | `[ { id, name } ]` | 3 |
| POST | `/play/playlist` | `{ id }` | `204` | 3 |
| GET | `/search` | `?q=&limit=25` | `[ SearchResult ]` (legacy, see "Legacy iTunes flow") | 3 |
| POST | `/play/track` | `{ url }` | `204` (legacy, see "Legacy iTunes flow") | 3 |
| GET | `/player/modes` | – | `Modes` | 4 |
| POST | `/player/modes` | `{ shuffle?, repeat? }` | `Modes` | 4 |
| GET | `/queue` | – | `{ items: [ QueueItem ] }` | 4 |
| POST | `/queue/play` | `{ index, title }` | `204` | 4 |
| GET | `/app-search` | `?q=&scope=catalog` | `{ results: [ AppSearchResult ] }` | 4 |
| POST | `/app-search/play` | `{ q, scope, section, index, title }` | `204` | 4 |

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

// Modes (MVP 4)
{ "shuffle": false, "repeat": "off" }     // repeat: "off" | "all" | "one"

// QueueItem (MVP 4). The current song is not in the list; it is in PlayerState.
{ "index": 0, "title": "…", "artist": "…", "album": "…",
  "duration_ms": 215000,                  // 0 if the app shows none
  "source": "queue" }                     // "queue" (Playing Next) | "autoplay"

// AppSearchResult (MVP 4). No link, id or artwork exists for these.
{ "section": "Songs", "index": 0,         // the section heading as the app shows it, and the position inside it
  "kind": "song",                         // song | album | artist | playlist | video | other
  "title": "…", "subtitle": "…" }         // subtitle: artist or curator, "" if none
```

### WebSocket `GET /ws?token=…`
Server → client messages: `{ "type": "<event>", "data": { … } }`
- `state` → `PlayerState` (sent on connect and when something changes; see "Behaviour notes")
- `volume` → `{ level, muted }`
- `apple_music` → `{ running: bool }`
- `modes` → `Modes` (MVP 4; sent on connect after `volume`, and when the agent sees a change)
Client → server: `{ "type": "ping" }` every 15 s; server replies `{ "type": "pong" }`.

### Errors
Non-2xx bodies: `{ "error": { "code": "…", "message": "…" } }`. `message` is for people; clients switch on `code` and treat an unknown code like `INTERNAL`.
Codes: `UNAUTHORIZED` (401), `BAD_PAIRING_CODE` (403), `BAD_REQUEST` (400), `NOT_FOUND` (404), `METHOD_NOT_ALLOWED` (405), `APPLE_MUSIC_NOT_RUNNING` (409), `NO_AUDIO_SESSION` (409), `NOT_SUPPORTED` (501), `INTERNAL` (500).
- `NO_AUDIO_SESSION`: Apple Music has a media session, but Windows has no audio session for it yet (nothing has played since it started, or it plays on another output device). Only `GET` and `POST /volume` answer it. Transport commands still work. A client disables the volume slider and does not show the "Open Apple Music" screen.
- `BAD_REQUEST`: malformed JSON, missing or wrong-typed field, wrong content type, value out of range.
- `NOT_FOUND`: unknown route, no artwork, unknown playlist id or song.
- `METHOD_NOT_ALLOWED`: known route, wrong HTTP method.
- Auth is checked before routing: a request without a valid token gets `401` for every path except `POST /pair`, even an unknown one.
- Each endpoint can answer `501` on its own (for example search works while playlists do not).

### Input rules
- `POST /volume`: `{}` is allowed and returns the current state. `level` must be an integer 0–100, otherwise `400` (not clamped). `muted` must be a boolean. An explicit `null` counts as not given. Changing `level` never changes `muted`.
- `POST /player/seek`: `position_ms` is an integer of 0 or more. The upper limit `duration_ms` applies only when `duration_ms` is greater than 0 (a duration of 0 means unknown, and any position is accepted). Otherwise `400`. `409` with no Apple Music session, `501` if Apple Music refuses the seek.
- `GET /search`: `q` is trimmed, 2–200 characters. `limit` defaults to 25 and must be 1–50, otherwise `400` (not clamped). Songs only, in catalog order, no paging. A space in `q` may be sent as `+` or `%20`. No internet on the laptop: `500 INTERNAL`.
- `POST /play/track`: `url` must be an `https://music.apple.com/…` song link containing `?i=<id>`, otherwise `400`. `409` if Apple Music is closed, `404` if the catalog does not know the song, `501` if Apple Music's window cannot be driven.
- `POST /play/playlist`: unknown `id` is `404`, `409` if Apple Music is closed, `501` if playlists cannot be read.
- `POST /player/modes`: both fields are optional and `{}` returns the current state. `shuffle` must be a boolean and `repeat` one of `"off"`, `"all"`, `"one"`, an explicit `null` counting as not given; otherwise `400`. The answer is the state read back from Apple Music after the change. `409` if Apple Music is closed, `501` if its shuffle or repeat buttons cannot be found, `500` if the state did not change.
- `GET /queue`: `409` if Apple Music is closed, `501` if the queue cannot be read.
- `POST /queue/play`: `index` is an integer of 0 or more and `title` a string that is non-empty after trimming, otherwise `400`. The agent matches on the trimmed title. `404` if there is no row at `index` or its title no longer matches (the queue moved on), `500` if a different song is current after the skips, `501` if the queue cannot be read, or for an `autoplay` row when the agent cannot play those, `409` if Apple Music is closed.
- `GET /app-search`: `q` is trimmed, 2–200 characters. `scope` is `catalog` (default) or `library`, otherwise `400`. No match is `200` with an empty list. `409` if there is no Apple Music window, `501` if its search cannot be driven, `500` if the app did not answer in time.
- `POST /app-search/play`: `q` (2–200 after trimming), `scope`, `section` and `title` (strings, non-empty after trimming) and `index` (integer 0 or more) are all required, otherwise `400`. The agent trims `section` and `title` and matches on the trimmed text. `404` if the search no longer finds that `title` at that `section` and `index`. `501` for a `kind` the agent cannot start yet (see "App search"), `409` if there is no Apple Music window.
- **Timeouts on the MVP 4 endpoints** (`/player/modes`, `/queue`, `/queue/play`, `/app-search`, `/app-search/play`): if Apple Music does not answer in time, the answer is `500 INTERNAL`, never `501`. `501` means the needed element cannot be found at all.
- **Order of checks:** `401` (auth) → `400` (malformed or invalid input) → `409`/`501` (Apple Music state) → `404` (lookup). A bad request therefore answers `400` even if Apple Music is not running. The one exception is the upper limit of `position_ms` on `/player/seek`, which needs the current duration and is checked after the `409`. `POST /play/track` is a second exception: it first looks the song up in the catalog (it needs the title), so `404` (unknown song) and `500` (laptop offline) can come before `409`. `POST /play/playlist` and the MVP 4 endpoints follow the normal order. `501 NOT_SUPPORTED` (UI Automation unavailable) is answered before `409`.
- A request with no JSON body at all, or the wrong content type, on `POST /volume`, `/player/seek`, `/play/track`, `/play/playlist` or `/pair` answers `400`. An empty or blank `id` on `/play/playlist` is `400`, not `404`. The same applies to the MVP 4 `POST` endpoints (`/player/modes`, `/queue/play`, `/app-search/play`).

### Behaviour notes
**Pairing**
- The code is single use and changes after each successful pair. 5 wrong codes lock `/pair` for 60 s, still answered `403 BAD_PAIRING_CODE`, even for the right code.
- `device_name` is optional. It is trimmed and cut to 64 characters; missing, `null`, not a string, or empty becomes `"Unknown device"`. It is never an error.
- `code` must be a string. A malformed request (invalid JSON, wrong content type, missing or non-string `code`) answers `400 BAD_REQUEST`. It does not count toward the 5 attempts, and it is answered `400` even while locked. Only a well-formed request with a wrong code counts, and while locked it answers `403 BAD_PAIRING_CODE`.
- `/status` needs a token. Only `POST /pair` is public.

**Apple Music state**
- `apple_music_running` (in `/status` and the `apple_music` event) is true only while Apple Music has a media session. Open but nothing played yet is false.
- Transport commands answer `409` without a session. With a session they answer `204`, even if Apple Music ignores the command (for example `next` at the end of the queue).
- If the agent's own media backend failed to start, `apple_music_running` is `false` and media calls fail. Which of `500`, `409` or `501` an endpoint answers is not specified; a client treats any non-2xx as failure.

**Volume**
- It is Apple Music's Windows per-app volume on the default output device (what the mixer shows). It is not the master volume and not Apple Music's in-app slider.
- With no audio session (nothing played since Apple Music started, or it plays on another device) `GET` and `POST /volume` answer `409 NO_AUDIO_SESSION`. With no media session at all they answer `409 APPLE_MUSIC_NOT_RUNNING`.
- A `volume` event follows within about 1 s of any change, wherever it was made.
- No `volume` event is sent when the audio session goes away (Apple Music quits, or the output device changes). A client treats its last level as stale when `apple_music` becomes false or a volume call answers `409`, and re-reads `GET /volume` when Apple Music is running again or after a reconnect.

**PlayerState**
- With nothing loaded or Apple Music closed, `GET /player/state` answers `200` with `status:"stopped"`, `title`, `artist` and `album` as `""`, `duration_ms` and `position_ms` as `0`, the current `artwork_rev`, and `playlist_name` as `null`.
- Only `playlist_name` can be `null`. A client maps a missing text or number field to `""` or `0`.
- `artist` and `album` are separate fields. If Apple Music gives `"Artist — Album"` as the artist with an empty album, the agent splits it at the first `" — "`.

**Artwork**
- The content type is the cover's own (`image/jpeg`, `image/png`, …), so clients must not assume JPEG.
- `404 NOT_FOUND` means no artwork. Any other error means try again later.
- `artwork_rev` increases on every track change, when a cover of the same track appears late, and when Apple Music quits. It never changes for pause, resume, seek or volume.

**WebSocket**
- On connect the agent sends `state`, then `apple_music`, then `volume` (only when there is an audio session), then `modes` (only when Apple Music is running and its shuffle and repeat buttons can be read).
- `state` is pushed on connect, and when `status`, `title`, `artist`, `album`, `duration_ms` or `artwork_rev` change, or when the position jumps by more than 2 s from what was expected (a seek or a skip, which also covers `POST /player/seek`). It is not sent every second: clients advance the position themselves while `status` is `playing`.
- A client silent for 45 s is disconnected, so keep the 15 s ping. Unknown client messages are ignored.
- A `GET /ws` without WebSocket upgrade headers answers `400 BAD_REQUEST`.
- A bad token on the handshake answers an HTTP `401` JSON body. Clients cannot read that status reliably, so they confirm with `GET /status`.
- A revoked token closes that device's socket at once.
- After a slow client lags behind, the agent resends `apple_music`, `state`, `volume` and `modes`.

**Search and playing**
- `artwork_url` is an https image link. A client may load it directly over HTTPS, without a token or other headers.
- Search uses the country of the Windows region setting, or `storefront` in the agent's `config.json` if set. `POST /play/track` uses the country of the link it is given.
- `POST /play/track` and `POST /play/playlist` answer `204` only after playback really started: normally 1–4 s, about 41 s in the worst case. Clients wait at least 45 s.
- They drive Apple Music's own window, so Apple Music visibly changes: its window goes to the song or playlist and the play queue changes. Nothing is typed or clicked with the mouse or keyboard.
- `/play/track`, `/playlists` and `/play/playlist` also work when Apple Music is open but nothing has played yet. They answer `409` only if there is no Apple Music window at all.
- A playlist `id` is an opaque string (Apple Music's own persistent id). `GET /playlists` lists the playlists the sidebar shows, not folders. Ids are not yet verified across an Apple Music restart, so clients refetch the list after a reconnect.

**Legacy iTunes flow (since C-006)**
- `GET /search` and `POST /play/track` (the iTunes Search API route) are legacy. They keep working unchanged in v1, but new clients use `/app-search`. They can only be removed together with a v2.

**Shuffle, repeat, queue and app search (MVP 4)**
- Apple Music gives no event for shuffle or repeat. The agent re-reads the two buttons about every 2 s while a client is connected and pushes `modes` on a change, so a change made on the laptop reaches the phone after a short delay. Shuffle also reorders the queue, so a client re-reads `GET /queue` after a modes change.
- `GET /queue` lists only what Apple Music shows: at most about the next 20 rows, `queue` rows first, then `autoplay` rows. It is not the whole queue. There is no stable row id, so a row is identified by `index` plus `title`. There is no `queue` event yet: a client re-reads `GET /queue` when `state` changes, after a modes change, and on request. The agent opens Apple Music's queue pane if it is closed and leaves it open.
- `POST /queue/play` skips forward `index + 1` times, so the songs in between play for a moment. It answers `204` once the target song is current (about 1–1.5 s).
- `GET /app-search` and `POST /app-search/play` drive Apple Music's own search box. The text is set through UI Automation, nothing is typed. A search takes about 4 s and a play 1–5 s, so clients wait at least 45 s. The window moves to the search and result pages and is not moved back.
- App-search results have no link, id or artwork. `section` is the heading the app shows (for example `Songs`, `Albums`, `Top Results`), `index` the position inside it. Play repeats the search and finds the item again by `section`, `index` and `title`. First version: library songs, catalog songs and catalog albums play; other kinds answer `501`. `scope=library` finds the user's own music.
- Calls that drive the Apple Music window run one after another. A second call waits for the first.
- The `modes` event is not guaranteed to follow every situation: it is skipped when the buttons cannot be read in time (about 1.5 s), and nothing is sent when Apple Music comes back with the same values. A client reads `GET /player/modes` on start, when `apple_music` becomes true, and after a reconnect, and does not wait for the event.
- A client treats an unknown `repeat` as `off` and an unknown `source` as `queue`, shows an unknown `kind` as given, and ignores unknown fields.
