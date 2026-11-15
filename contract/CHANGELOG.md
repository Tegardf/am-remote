# Contract changelog

Only the **root window** writes this file, via `/contract-change`.
Each project applies new entries with `/sync-contract` in its own window, then records the newest id it applied in its own `CLAUDE.md` (the "Contract changes applied:" line).

Newest entry first. Entry format:

```
## C-NNN · YYYY-MM-DD · <title>
- **Applies to:** agent, app
- **Breaking:** no
- **Change:** <what changed in the contract section of CLAUDE.md>
- **Resolves:** <project requests / open question numbers, or "-">
- **Agent notes:** <hints for desktop-agent, or "-">
- **App notes:** <hints for android-controller, or "-">
```

---

## C-007 · 2026-11-15 · MVP 4 clarifications (contract freeze for v0.2.0)
- **Applies to:** agent (one small fix); app (marker only)
- **Breaking:** no (clarifications of what the code already does; one rare status code is pinned: an Apple Music timeout on an MVP 4 endpoint is `500`, not `501`)
- **Change:** Input rules: a timeout on any MVP 4 endpoint is `500 INTERNAL`, `501` only when the element is missing; `title` and `section` are non-empty after trimming and matched trimmed. Behaviour notes: the `modes` event is not guaranteed (skipped on a read timeout, not resent when Apple Music comes back unchanged), so clients read `GET /player/modes` on start, when `apple_music` becomes true, and after a reconnect; clients map an unknown `repeat` to `off`, an unknown `source` to `queue`, show an unknown `kind` as given, and ignore unknown fields.
- **Resolves:** conformance report gaps "timeout code for window-driving calls", "unknown values on the client side", "blank and trimmed text" and "`modes` after Apple Music comes back"; conformance finding #1 (agent).
- **Agent notes:** map the outer UI Automation timeout in `Uia::ask` (`src/media/uia.rs`, `OP_TIMEOUT`) to `MediaError::Internal` (`500`) instead of `NotSupported`, and add a test (this is conformance finding #1). Nothing else changes.
- **App notes:** no contract change (the app already does all of this). Fix conformance finding #2 (count `q` with `.runes` for the 2-character minimum, plus a one-emoji test) and record the marker C-007.
- **Freeze:** the v1 contract is frozen again until v0.2.0 is tagged; see `contract/RELEASE.md`.

---

## C-006 · 2026-11-15 · MVP 4: shuffle/repeat, queue, app search
- **Applies to:** agent, app
- **Breaking:** no (additive: new endpoints and one new WebSocket event; nothing existing changes)
- **Change:** New endpoints `GET`/`POST /player/modes` (`Modes`: `shuffle` bool, `repeat` `off|all|one`), `GET /queue` (`QueueItem`, about the next 20 rows), `POST /queue/play` (`index` + `title`), `GET /app-search` (`q`, `scope=catalog|library`, `AppSearchResult`) and `POST /app-search/play`. New WebSocket event `modes` (on connect after `volume`, and on change). Input rules and behaviour notes for all of them. `GET /search` and `POST /play/track` are marked legacy; they stay in v1 and can only be removed with v2. Seven new samples: `modes_response`, `modes_request`, `queue_response`, `queue_play_request`, `app_search_response`, `app_search_play_request`, `ws_modes`.
- **Resolves:** the MVP 4 request (shuffle and repeat buttons, scrollable queue with tap to play, search through the Apple Music app). Source: `desktop-agent/docs/spikes/mvp4.md`.
- **Agent notes:** everything goes through UI Automation behind the `uia` feature (`501` without it or when an element is missing). Shuffle: `ShuffleButton` (Toggle). Repeat: `RepeatButton` (Invoke cycles off → all → one; read the state from its Name, at most 2 invokes). SMTC shuffle/repeat calls do nothing, do not use them. Queue: pane via `PlayQueueToggleButton`, rows in `PlayQueueListView` then `PlayQueueListViewAutoplay`; play a row with `index + 1` SMTC `Next` calls after re-checking the title. Search: `SetValue` on the sidebar search box, invoke the suggestion named exactly like the query, results by section; library song `Invoke` plays, catalog album/song `Invoke` then the page's Play button or row menu. Poll modes about every 2 s, re-read the queue on `StructureChanged` and `MediaPropertiesChanged`. Serialize window-driving calls. Verify autoplay-row skipping; answer `501` for it if it fails. Add the seven samples to the contract test.
- **App notes:** shuffle and repeat buttons (state from `modes`); an Up Next list under the player buttons, scrollable, tap a row to call `POST /queue/play`, re-read `GET /queue` on `state` and `modes` changes; search screen uses `/app-search` (with a catalog/library switch) and `POST /app-search/play`, no artwork or link; stop using `/search` and `/play/track`; wait 45 s for the play calls. Add the seven samples to the contract test.

---

## C-005 · 2026-11-14 · Final v1 clarifications (contract freeze)
- **Applies to:** none (documentation only; it describes what the code already does)
- **Breaking:** no
- **Change:** Order of checks: `POST /play/track` looks the song up first, so `404` and `500` can come before `409`; `501` is answered before `409`. If the agent's media backend failed to start, `apple_music_running` is `false` and media calls fail with an unspecified `500`, `409` or `501`; clients treat any non-2xx as failure. Discovery: the resolver-IPv4 fallback applies to a saved (paired) session; when pairing from the discovery list a client may try only the `ip` host.
- **Resolves:** conformance report gaps "`POST /play/track` and the order of checks", "`501` versus `409`", "what counts as a media call when the backend failed" and "first-pairing fallback".
- **Agent notes:** no code change. Record the marker C-005 with `/sync-contract`.
- **App notes:** no code change. Record the marker C-005 with `/sync-contract`.
- **Freeze:** the v1 contract is frozen until the release is tagged; see `contract/RELEASE.md`.

---

## C-004 · 2026-11-14 · ip refresh, /pair body rules, order of checks
- **Applies to:** agent, app
- **Breaking:** no (clarifications; one rule is loosened: a missing `device_name` is now accepted instead of `400`)
- **Change:** Discovery: the agent re-publishes TXT `ip` when its LAN IPv4 changes, and a client that cannot reach `ip` falls back to a resolver IPv4 and re-runs discovery. Pairing: `device_name` is optional (missing, `null`, non-string or empty becomes "Unknown device"); a malformed `/pair` request answers `400`, does not count toward the 5-attempt lockout, and is answered `400` even while locked. Input rules: order of checks (`401` → `400` → `409`/`501` → `404`, with the seek upper limit after the `409`); a missing JSON body or wrong content type is `400`; an empty or blank playlist `id` is `400`.
- **Resolves:** conformance report gaps "`ip` in the TXT record can go stale", "`POST /pair` bodies" and "which error wins".
- **Agent notes:** (1) re-publish the TXT `ip` when the primary IPv4 changes (a check every ~30 s is enough) and add a test for the refresh; (2) make `device_name` optional in `PairRequest` (`pair.rs:25-28`) and add a test with a missing and a `null` name. The malformed-body, lockout, order-of-checks, empty-body and empty-`id` rules already match the code.
- **App notes:** when the `ip` host is unreachable, fall back to an IPv4 address the resolver gave it, then re-run discovery (`SessionController._rediscover`); add a test for the fallback. Nothing else changes.

---

## C-003 · 2026-11-14 · NO_AUDIO_SESSION, volume staleness, seek with unknown duration
- **Applies to:** agent, app
- **Breaking:** no in practice. The code on `GET`/`POST /volume` with no audio session changes from `APPLE_MUSIC_NOT_RUNNING` to the new `NO_AUDIO_SESSION` (still `409`). Only these two unreleased clients exist, and an older client treats an unknown code as `INTERNAL`.
- **Change:** New error code `NO_AUDIO_SESSION` (409), answered only by `GET`/`POST /volume`; no media session at all is still `APPLE_MUSIC_NOT_RUNNING`. Volume note: no `volume` event when the audio session goes away, and how clients treat a stale level. Seek: `duration_ms` of 0 means unknown, so any position of 0 or more is accepted. Sample `error_no_audio_session` added.
- **Resolves:** conformance report gaps "two different 409 situations", "volume when the audio session disappears" and "seek when duration_ms is 0"; it also replaces the conformance finding about the volume 409 message text.
- **Agent notes:** answer `GET`/`POST /volume` with `NO_AUDIO_SESSION` (409, message "Apple Music has no audio session yet"; `MediaError::NoAudioSession` already has this text) and update the stale comment and test; keep accepting any seek position when `duration_ms` is 0; add `error_no_audio_session` to the contract test.
- **App notes:** recognise `NO_AUDIO_SESSION`; on it disable only the volume slider, keeping the playback buttons and now-playing view, and do not show the "Open Apple Music" screen; treat the last volume as stale per the Volume note and re-read `GET /volume` after a reconnect or when Apple Music is running again; add the new sample to the contract test.

---

## C-002 · 2026-11-14 · Behaviour notes, ip TXT field, WebSocket on connect
- **Applies to:** agent, app
- **Breaking:** no (additive; clients that ignore the new TXT field and extra WebSocket events keep working)
- **Change:** New TXT field `ip=<ipv4>` and rules for choosing the host. New "Behaviour notes" section: pairing, Apple Music state, volume, PlayerState, artwork, WebSocket, search and playing. On connect the WebSocket now sends `state`, `apple_music`, then `volume`. `/player/artwork` is `image/*`, not only JPEG. `artwork_url` may be loaded directly over HTTPS.
- **Resolves:** open questions 2, 3, 4 and 5 (the open-questions list is now empty); all remaining desktop-agent and android-controller requests in their `docs/contract-requests.md` files.
- **Agent notes:** (1) add `ip=<ipv4>` to the mDNS TXT record: the laptop's primary LAN IPv4 address; (2) on WebSocket connect send `apple_music`, then `volume` (only with an audio session) after `state`. Everything else already matches. Tick all resolved requests, set the applied marker to C-002, and extend the WebSocket test for the connect order.
- **App notes:** (1) prefer TXT `ip` as the host, else an IPv4 address from the resolver, else skip the agent; (2) HTTPS artwork loads stay allowed, so remove the "no internet traffic" line from `android-controller/CLAUDE.md`; (3) after a reconnect, take the `apple_music` and `volume` events from the connect burst, and keep the 45 s request timeout for play-track and play-playlist. Tick all resolved requests and set the applied marker to C-002.

---

## C-001 · 2026-11-14 · Error codes and input rules
- **Applies to:** app (agent already behaves this way)
- **Breaking:** no (additive: three new error codes, rules written down)
- **Change:** Added error codes `BAD_REQUEST` 400, `NOT_FOUND` 404, `METHOD_NOT_ALLOWED` 405. Auth runs before routing. New "Input rules" section for `POST /volume`, `POST /player/seek`, `GET /search`, `POST /play/track`, `POST /play/playlist`. Samples `error_bad_request`, `error_not_found`, `error_method_not_allowed` added.
- **Resolves:** open question 1; desktop-agent requests "BAD_REQUEST code", "NOT_FOUND / METHOD_NOT_ALLOWED codes", "401 before routing", "POST /volume rules", "POST /volume explicit null", "/player/seek rules", "/search rules", "/play/track rules", "404 NOT_FOUND as a contract code"; android-controller requests "POST /volume level and {}", "failure answers of playlists/search/play endpoints", "/search results, limit and q encoding"
- **Agent notes:** nothing to change; the agent already matches. Tick the resolved requests in docs/contract-requests.md, set the applied marker to C-001, and make the contract test cover the three new error samples.
- **App notes:** recognise `BAD_REQUEST`, `NOT_FOUND` and `METHOD_NOT_ALLOWED` in the error-code mapping (today they become `INTERNAL`); show a specific message for `NOT_FOUND` on play-track and play-playlist (item no longer available). Cover the three new samples in the contract test.

---

## C-000 · 2026-11-14 · Contract v1 baseline
- **Applies to:** agent, app
- **Breaking:** no
- **Change:** The initial v1 contract as written in `CLAUDE.md`, with examples in `contract/samples.md`.
- **Resolves:** -
- **Agent notes:** Nothing to apply on its own. The contract is built phase by phase with `/build-mvp`.
- **App notes:** Same as the agent.
