# MVP 4 — Shuffle/repeat, queue & app search (desktop-agent)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
Implement contract C-006: read and set shuffle and repeat, read the Up Next queue and play a row, and search through Apple Music's own search box.

## Gate
- `docs/spikes/mvp4.md` exists with a `**Decision:**` line. Read its sections A, B and C first: every selector and timing you need is there. Otherwise stop and suggest `/spike mvp4`.
- `../contract/CHANGELOG.md` has C-006. If not, stop and ask for `/contract-change` in the root window.
- MVP 3 is ticked and C-006 is the next change to apply (run `/sync-contract` to record it when done).

## Build
All of it is UI Automation in `media/uia.rs`, behind the `uia` feature. Without it, or when an element is missing, answer `501 NOT_SUPPORTED`. Never guess another element, never simulate keys or the mouse, and put a timeout on every call. Keep each selector in one const commented `// verified: spike mvp4, 2026-11-14`.
- [ ] **Window lock:** one queue for everything that drives the Apple Music window (MVP 3 calls too), so two requests never run together.
- [ ] **Modes:** `ShuffleButton` (TogglePattern, 0/1) and `RepeatButton` (InvokePattern; its Name `Do Not Repeat` → `Repeat All` → `Repeat One` is the state, at most 2 invokes to reach a target). Do not use SMTC for this; it accepts the calls and does nothing. `GET`/`POST /player/modes` answer the state read back; `500` if it did not change after about 1.5 s.
- [ ] **Modes event:** poll the two buttons about every 2 s while a WebSocket client is connected, scoped to the transport bar. Push `modes` on connect (after `volume`, only when readable), on change, and in the lag resend.
- [ ] **Queue read:** `PlayQueueToggleButton` opens the pane if it is closed (leave it open). The pane is the `PaneRoot` that contains the "Playing Next" toggle (there are two `PaneRoot`s). Rows of `PlayQueueListView` (`source: "queue"`) then `PlayQueueListViewAutoplay` (`"autoplay"`), at most 20 in total, `index` counting across both. Fields from the three `TextBlock`s: title, `"<artist> — <album>"` (split at the first ` — `), `"m:ss"` (→ `duration_ms`, 0 if missing). The current song is not a row.
- [ ] **Queue play:** re-read the rows, check `title` at `index` (`404` if not), then call SMTC `Next` `index + 1` times (100–300 ms apart) and wait for the title to become current (up to about 5 s). Different song current → `500`. Try an `autoplay` row once on the real app; if skipping through it fails, answer `501` for `autoplay` rows and say so in the smoke test.
- [ ] **App search:** `Search_Button` → the `TextBox` in the `AutoSuggestBox` (`ValuePattern.SetValue`, no typing) → wait ~1 s, set it again if no suggestion shows within ~2 s → invoke the `SuggestionsList` item named exactly like the query (or, if the results page is already open, wait for `SearchPageStatus` to say `Showing results for “<q>”`). Set `scope` with the `SearchAppleMusic` / `SearchLibrary` toggles. Read sections (scroll the page `ListView` with `SetScrollPercent` to realize later ones) into `AppSearchResult { section, index, kind, title, subtitle }`; kind from the label in the name (`Song ·`, `Artist`, `Album ·`, `Playlist ·`) or the section. Empty result is `200` with `[]`; no answer in time → `500`.
- [ ] **App search play:** repeat the search, find `title` at `section` and `index` (`404` if not), then: library song → `Invoke` (plays at once); catalog album → `Invoke`, then `PlayButtonElement` on the page; catalog song → `Invoke`, then the MVP 3 row route or the Single's `PlayButtonElement`. Other kinds → `501`. Use the same 45 s ceiling as `/play/track`. After playing, leave the window where it is.
- [ ] **Queue event:** none in this version. Do not add one.
- [ ] Add the new routes to the router with the normal order of checks (`401 → 400 → 501 → 409 → 404`) and the input rules in the contract.

## Tests (required)
- Round-trip the samples `modes_response`, `modes_request`, `queue_response`, `queue_play_request`, `app_search_response`, `app_search_play_request` and `ws_modes`, and keep `every_sample_is_covered` green.
- Pure rules, no Apple Music: repeat-state-from-name and the number of invokes to reach a target; row text → `QueueItem` (artist/album split, `m:ss`, a missing duration); result name → kind; input validation (`repeat`, `scope`, `index`, blank `title`, `q` length) → `400` before anything else.
- With the fake `MediaBackend`: `modes` pushed on connect and on change, not when nothing changed; `409` when Apple Music is closed; `501` with the `uia` feature off for all six new endpoints (`cargo test --no-default-features`); window-driving calls run one at a time.
- Queue play with the fake: a moved queue → `404`; a wrong landing song → `500`.

## Done when
- Step 3 is green, both with and without `uia`.
- With the real app (needs a human): shuffle and repeat work from the API and from the laptop, the queue lists and plays a row, a library song and a catalog album play from app search.

## Smoke test (→ docs/smoke-tests/mvp-4.md)
1. `POST /player/modes {"shuffle":true,"repeat":"all"}` → the app's buttons match, answer matches. Set both back off.
2. Press shuffle on the laptop → a `modes` event reaches a WebSocket client within a few seconds.
3. `GET /queue` with the pane closed → the pane opens, up to 20 items, none is the current song.
4. `POST /queue/play` for row 2 with its title → that song plays within ~1.5 s and the rest of the queue is unchanged. Same call with a wrong title → `404`. Try one `autoplay` row and note the result.
5. `GET /app-search?q=<your artist>&scope=library`, play the first song → it plays at once.
6. `GET /app-search?q=<artist>&scope=catalog`, play an album → it starts from track 1 (allow ~5 s).
7. Close the queue pane and rename nothing: the queue call reopens it. Minimize Apple Music → note what answers (`409`/`501`).
8. Update or break a selector on purpose (use a wrong id in a debug build) → the endpoint answers `501`, no hang.

## Next
Run `/sync-contract` to record C-006, then `/verify`. Then the android-controller window builds its MVP 4, and the root window runs `/contract-check` once at the end.
