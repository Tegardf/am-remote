# MVP 1 — Player controls (android-controller)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
Play/pause, next, previous, a volume slider and mute, all controlling the laptop.

## Gate
- MVP 0 is ticked, or its leftovers are first on the todo list.

## Build
- [ ] `core/models/volume.dart`: `Volume { level, muted }`, plus request bodies for `level` only, `muted` only, and both.
- [ ] `api_client`: `play()`, `pause()`, `toggle()`, `next()`, `previous()` (each a POST expecting 204); `getVolume()` and `setVolume({level, muted})`.
- [ ] `features/player/`: big play/pause toggle, next and previous buttons. Send the command and don't change the UI optimistically (MVP 2's push will drive state). Disable the buttons while a request is in flight.
- [ ] Volume slider: moves locally while dragging and sends `POST /volume` debounced (~100 ms), with a final send on release. Mute toggle button. Load the initial value with `GET /volume`.
- [ ] `APPLE_MUSIC_NOT_RUNNING` (409) → an "Open Apple Music on your laptop" empty state with a retry button.
- [ ] Nice-to-have: hardware volume keys control the laptop volume while the Player screen is in front. If it needs platform code you can't test, skip it and say so.

## Tests (required)
- Round-trip `volume_response`, `volume_request_level`, `volume_request_muted` and `volume_request_both`.
- Widget: tapping each button calls the matching api method once (fake client).
- Slider debounce with fake time: 10 quick changes → 1-2 POSTs, and the last value is always sent.
- 409 → the empty state is shown; retry calls the API again.

## Done when
- Step 3 is green.
- With the agent's MVP 1, the buttons and the slider control Apple Music on the laptop (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-1.md)
1. Paired, with Apple Music playing on the laptop.
2. Play/pause, next and previous each act within ~1 s.
3. Drag the slider: the laptop volume follows smoothly with no flood of lag. Release at 30 → the laptop mixer shows 30.
4. Mute → only Apple Music is muted; unmute restores it.
5. Close Apple Music → the empty state appears. Reopen it and tap retry → the controls are back.

## Next
`/build-mvp 2`
