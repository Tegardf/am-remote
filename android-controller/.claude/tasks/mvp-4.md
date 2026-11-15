# MVP 4 — Shuffle/repeat, queue & app search (android-controller)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
Implement contract C-006 in the app: shuffle and repeat buttons, a scrollable Up Next list under the player buttons (tap a row to play it), and a search screen that uses the Apple Music app's own search.

## Gate
- `../contract/CHANGELOG.md` has C-006. If not, stop and ask for `/contract-change` in the root window.
- MVP 3 is ticked. The agent need not be finished: the app must cope with `501` and with an agent that does not know the new routes (`404`).

## Build
- [ ] `core/models/`: `Modes { shuffle, repeat }` (repeat is an enum `off|all|one`, an unknown value maps to `off`), `QueueItem`, `AppSearchResult` (match the contract exactly).
- [ ] `api_client`: `getModes()`, `setModes({shuffle, repeat})`, `getQueue()`, `playQueueItem(index, title)`, `appSearch(q, scope)`, `playAppResult(q, scope, section, index, title)`. The two play calls wait 45 s like `playTrack`.
- [ ] `ws_client` / player controller: handle the `modes` event; take the connect burst as before. Keep the last modes as stale and re-read `GET /player/modes` after a reconnect or `409`.
- [ ] **Shuffle and repeat buttons** on the Player screen next to the transport buttons. Shuffle toggles; repeat cycles off → all → one with a distinct icon for each. Like the other buttons they send the command and show the answer or the push, not a guess. `501` → disable only these two buttons with a short note ("Not available on this laptop"). `409` → the usual "Open Apple Music" state.
- [ ] **Up Next list** below the action buttons on the Player screen: scrollable, title and artist per row, `autoplay` rows under a small "AutoPlay" divider. Tap a row → `playQueueItem(index, title)` and show a short busy state (about 1–2 s). `404` → refresh the list with a snackbar "The queue changed". The current song is shown by the now-playing card, not in the list. Note that the list shows about the next 20 songs.
- [ ] **Refresh rules:** read `GET /queue` when the Player screen opens, when `state` changes `title` or `artwork_rev`, after a modes change, and on pull-to-refresh. Never poll on a timer. `501` → "Queue isn't available on this laptop", no error spam.
- [ ] **Search screen:** switch to `/app-search`. A Catalog / Library switch (default Catalog), field debounced 300 ms with at least 2 characters, results grouped by `section` with the title and subtitle, no artwork, no link. Search takes about 4 s: show a clear loading state and wait up to 45 s. Tap a result → `playAppResult(...)` → Player tab. `501` on play for an unsupported kind → "This kind of result can't be played from the phone yet". Stop using `search()` and `playTrack()` in the UI (leave the client methods for now).
- [ ] Show the side effect once, as a small line on the Search screen: "Searching opens the search page in Apple Music on the laptop."

## Tests (required)
- Round-trip `modes_response`, `modes_request`, `queue_response`, `queue_play_request`, `app_search_response`, `app_search_play_request` and `ws_modes`, and add the missing guard so a new sample key fails the test.
- Modes: a `modes` event updates the buttons; an unknown `repeat` string becomes `off`; `501` disables only the two buttons.
- Queue: refreshed on a `state` title change and after a modes change, not on a timer; tapping row N calls `playQueueItem` with that row's index and title; `404` triggers a refresh; `501` shows the explanation state.
- Search: debounce with fake time ("ab" quickly → 1 request, "a" → none); the scope switch re-searches; tapping a result sends `q`, `scope`, `section`, `index` and `title` unchanged; a 45 s wait does not time out earlier.

## Done when
- Step 3 is green.
- With the agent's MVP 4 (needs a human): both buttons work and follow changes made on the laptop, the queue list scrolls and plays a row, library and catalog search play.

## Smoke test (→ docs/smoke-tests/mvp-4.md)
1. Tap shuffle and repeat → the buttons change and the laptop's buttons match. Change them on the laptop → the phone follows within a few seconds.
2. The Up Next list shows the next songs below the buttons and scrolls. Tap row 3 → that song plays within about 2 s and the list moves on.
3. Skip a track on the laptop → the list updates. Turn shuffle on → the list is re-read and reordered.
4. Search tab, Library: type one of your artists → songs found; tap one → it plays at once.
5. Search tab, Catalog: type an artist → grouped results; tap an album → it plays from track 1 (allow ~5 s). The loading state stays visible meanwhile.
6. Tap a result kind the agent cannot play → the friendly message, no crash.
7. Close Apple Music → the usual "Open Apple Music" state on all three parts; reopen → everything recovers.

## Next
Run `/sync-contract` to record C-006, then `/verify`. After both projects are done, the root window runs `/contract-check` once.
