# MVP 2 — Now playing & live push (desktop-agent)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
The phone always knows what's playing: player state, artwork, seek, and live updates over WebSocket.

## Gate
- MVP 1 is ticked, or its leftovers are first on the todo list.
- Optional: run `/spike uia-queue` first. Without a **GO** in `docs/spikes/uia-queue.md`, `playlist_name` stays `null`.

## Build
- [ ] `GET /player/state` → `PlayerState`. Map SMTC playback status to `playing|paused|stopped|unknown`.
- [ ] Live position = `position + (now - LastUpdatedTime)` while playing, clamped to `duration_ms`. Convert TimeSpan (100 ns ticks) to ms inside the media module.
- [ ] Artwork: `Thumbnail()` → `OpenReadAsync` → bytes, cached per track. Bump `artwork_rev` when the track changes. `GET /player/artwork` → `image/jpeg`, or 404 (contract error body) if there's none.
- [ ] `POST /player/seek { position_ms }` → `TryChangePlaybackPositionAsync` → 204. Out of range → validation error.
- [ ] Events: subscribe to `MediaPropertiesChanged`, `PlaybackInfoChanged` and `TimelinePropertiesChanged` on the session, and to `SessionsChanged` on the manager. When Apple Music restarts, re-subscribe. Push changes into a `tokio::sync::broadcast` channel.
- [ ] Volume changes: poll the Core Audio session (~1 s) or use `IAudioSessionEvents`. Emit a `volume` event on change.
- [ ] `GET /ws?token=…` (`api/ws.rs`): on connect send `state`. Then forward `state`, `volume` and `apple_music { running }`. Answer `{ "type": "ping" }` with `{ "type": "pong" }`, ignore unknown client messages, and close the socket when its token is revoked.
- [ ] `playlist_name`: `null`, unless the uia-queue spike says GO. If it does, implement it in `media/uia.rs` behind the `uia` cargo feature, falling back to `null` on any failure.

## Tests (required)
- Round-trip the samples `player_state_playing`, `player_state_paused_in_playlist`, `seek_request` and every `ws_*`.
- Position interpolation: a pure function tested with fixed instants.
- WebSocket (axum test server on localhost with the fake backend): `state` arrives first; a backend change produces a new `state`; ping → pong; a bad token is refused.
- Artwork: 404 body when there's none; `artwork_rev` increases only when the track changes.

## Done when
- Step 3 is green.
- A WebSocket client (e.g. `wscat` or the app) shows `state` updating when you skip a track in Apple Music (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-2.md)
1. Connect to `ws://<ip>:8765/api/v1/ws?token=…` → a `state` message arrives immediately.
2. Skip a track in Apple Music itself → a new `state` arrives within ~1 s, with a higher `artwork_rev`.
3. `GET /player/artwork` shows the cover (open it in a browser with the token header, or via the app).
4. `POST /player/seek {"position_ms":60000}` → Apple Music jumps to 1:00.
5. Change the volume in the Windows mixer → a `volume` event arrives.
6. Quit Apple Music → `apple_music {running:false}`. Restart it and play → events resume without restarting the agent.
7. Revoke the token in the tray → the socket closes.

## Next
`/spike link-playback`, then `/build-mvp 3`.
