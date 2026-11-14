# MVP 2 — Now playing & live updates (android-controller)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
A live now-playing screen (title, artist, album, artwork, progress, seek), driven by WebSocket pushes, with controls in a media notification and on the lock screen.

## Gate
- MVP 1 is ticked, or its leftovers are first on the todo list.

## Build
- [ ] Packages: `web_socket_channel`, `audio_service` (and `cached_network_image` only if it's needed).
- [ ] `core/models/`: `PlayerState` (`status` with `unknownEnumValue: unknown`, int ms fields, `artwork_rev`, nullable `playlist_name`) and `WsMessage { type, data }`.
- [ ] `core/ws_client.dart`: connect to `ws://<host>:<port>/api/v1/ws?token=…`. Send `{ "type": "ping" }` every 15 s; no `pong` within ~10 s → reconnect. Exponential backoff (1 s → 30 s max). Expose streams of `state`, `volume` and `apple_music`, and ignore unknown types.
- [ ] WebSocket `state` is the source of truth for the Player UI (the buttons still just send HTTP).
- [ ] Now-playing card: title, artist and album. Fetch artwork via `GET /player/artwork` (with the auth header) only when `artwork_rev` changes; on 404, show a placeholder.
- [ ] Progress bar: interpolate locally every ~250 ms while `playing`, re-sync on every `state`. Dragging seeks with `POST /player/seek` on release.
- [ ] `volume` events update the slider, except while the user is dragging. An `apple_music {running:false}` event shows the empty state.
- [ ] `audio_service`: a handler that mirrors `PlayerState` into the media notification and lock screen; its play, pause, next and previous call the API. Manifest: `FOREGROUND_SERVICE` and `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, plus the service and receiver entries from the `audio_service` docs. The phone plays no audio.
- [ ] Offline banner on network loss. If the saved host fails repeatedly, re-run discovery.

## Tests (required)
- Round-trip `player_state_playing`, `player_state_paused_in_playlist`, `seek_request` and every `ws_*`. An unknown `status` value → `unknown`.
- `ws_client` with a fake channel: ping every 15 s (fake time), reconnect after a missing pong, unknown types ignored.
- Artwork is re-fetched only when `artwork_rev` changes.
- Progress interpolation: a pure function tested with fixed times.

## Done when
- Step 3 is green.
- With the agent's MVP 2, changes made on the laptop appear on the phone within ~1 s, and the notification controls work (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-2.md)
1. Open the app → the current song appears with artwork; the progress bar moves.
2. Skip a track **on the laptop** → the phone updates within ~1 s, with new artwork.
3. Seek on the phone → the laptop jumps there.
4. Lock the phone → the lock screen shows the song with working play/pause/next/previous.
5. Turn the phone's Wi-Fi off for 30 s and back on → the offline banner shows, then it reconnects by itself.
6. Restart the agent → the app reconnects without being re-paired.

## Next
`/build-mvp 3` (on the agent side, `/spike link-playback` must have run first).
