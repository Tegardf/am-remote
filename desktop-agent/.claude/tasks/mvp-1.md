# MVP 1 — Playback controls (desktop-agent)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
Play, pause, toggle, next, previous and volume/mute of the Apple Music app, through the API.

## Gate
- MVP 0 is ticked in ./CLAUDE.md, or its leftovers are first on the todo list.
- Apple Music is installed. For the real-machine probes it must be running and have played something once.

## Build
- [ ] **Media thread:** one dedicated thread owns every WinRT/COM object. It calls `CoInitializeEx(COINIT_MULTITHREADED)` and receives commands over a channel, replying through oneshot channels. Handlers never block the tokio runtime.
- [ ] **AUMID probe:** `examples/list_sessions.rs` prints every SMTC session's `SourceAppUserModelId`. Put Apple Music's id in one const (`// verified: …` or `// UNVERIFIED`).
- [ ] `media/smtc.rs`: `GlobalSystemMediaTransportControlsSessionManager` → find the session whose AUMID matches. Never control another app's session.
- [ ] `POST /player/play`, `/pause`, `/toggle`, `/next`, `/previous` → `204`. With no Apple Music session → `409 APPLE_MUSIC_NOT_RUNNING`.
- [ ] **Process probe:** find the Apple Music process name (`AppleMusic.exe`? verify it). Keep it in one const.
- [ ] `media/audio.rs`: default render device → `IAudioSessionManager2` → the session whose PID matches → `ISimpleAudioVolume`. Map 0.0-1.0 ↔ 0-100 at this edge only.
- [ ] `GET /volume` → `{ level, muted }`; `POST /volume { level?, muted? }` → the new `{ level, muted }`. If `level` is outside 0-100, return the validation error from MVP 0.
- [ ] No audio session yet (Apple Music hasn't played since it started): follow open question #3 in `../contract/README.md`. If it's unsettled, return `409 APPLE_MUSIC_NOT_RUNNING` and log a request.
- [ ] `GET /status` reports `apple_music_running` for real.

## Tests (required)
- Round-trip the samples `volume_response`, `volume_request_level`, `volume_request_muted` and `volume_request_both`.
- With the fake backend: each player command → 204; backend "not running" → 409 with the error body.
- Volume mapping: 0 ↔ 0.0 and 100 ↔ 1.0, with rounding at 0.5. Out-of-range values are rejected.
- `POST /volume` with only `muted` leaves `level` unchanged.

## Done when
- Step 3 is green.
- With Apple Music playing, the README's `Invoke-RestMethod` calls toggle playback and change the volume (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-1.md)
1. Apple Music is playing, and the agent is running and paired.
2. `POST /player/toggle` pauses; calling it again resumes. `next` and `previous` change the track.
3. `POST /volume {"level":20}`: Apple Music gets quieter, and the Windows volume mixer shows it at 20 % of the master. Other apps don't change.
4. `POST /volume {"muted":true}` mutes only Apple Music.
5. Close Apple Music → commands return 409 `APPLE_MUSIC_NOT_RUNNING`, and `/status` shows `apple_music_running: false`.
6. Play another app (e.g. a YouTube tab in a browser) and call `toggle` → it affects Apple Music only (or returns 409).

## Next
`/build-mvp 2`
