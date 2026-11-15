# Spike: mvp4
**Decision:** A shuffle/repeat: **UIA** (SMTC: NO) · B queue: **PARTIAL** (read: GO, play a row: via SMTC Next, list capped at 20) · C app search: **PARTIAL** (library: GO, catalog: GO by name with an extra step, links/ids/artwork: NO) · **Confidence:** high for what was observed (every claim below was run on the real machine, mostly several times); medium for fragility across languages, window modes and app updates, which were not tried · **Date:** 2026-11-14 · **Apple Music version:** 1.1792.39942.0 (English UI, main window, queue pane open)

Probes (all in `examples/`, all use public UI Automation patterns and SMTC; no keyboard or mouse input was simulated): `spike_mvp4_shuffle` (SMTC), `spike_mvp4_uia_shuffle`, `spike_mvp4_find` (element finder), `spike_mvp4_queue` (`read`, `summary`, `toggle-pane`, `scroll`, `scroll-end`, `patterns`, `play`, `next`, `skip-to`, `shuffle-order`), `spike_mvp4_events`, `spike_mvp4_search` (`open`, `set`, `flow`, `sections`, `scrollpage`, `scrollto`, `toggle`, `invoke`, `props`, `back`, `top`), shared helper `examples/spike_common/`. Dev-dependency changes for them: windows feature `Media`, crate `windows-core` (already in the tree).

Side effects of running the probes, all restored at the end: music was interrupted and several playlists, songs and an album were played; shuffle and repeat were toggled and put back (shuffle off, "Do Not Repeat"); the queue pane was closed and reopened (open again); the app was left on a song page, with the sidebar search box holding the last query.

## A. Shuffle and repeat
**SMTC: NO.** `PlaybackInfo.IsShuffleActive` and `AutoRepeatMode` are not reported (both `None`), and `Controls.IsShuffleEnabled` / `IsRepeatEnabled` are `false`. `TryChangeShuffleActiveAsync(true|false)` and `TryChangeAutoRepeatModeAsync(List|Track|None)` all return `Ok(true)` ("accepted") but nothing changes in the app and the state stays unreported. Same pattern as seek in MVP 2: accepted, no effect.

**UI Automation: yes.** Both buttons are in the transport bar (`TransportBar`) and have stable AutomationIds.
- **Shuffle:** `ShuffleButton`, class `ToggleButton`, `TogglePattern`: state `0` = off, `1` = on. Toggling it twice returned to the start state. (Do not confuse it with `ShuffleButtonElement`, a page's "play shuffled" button.)
- **Repeat:** `RepeatButton`, class `Button`, `InvokePattern` only. Each invoke cycles the state, and the **Name** carries it: `"Do Not Repeat"` → `"Repeat All"` → `"Repeat One"` → `"Do Not Repeat"` (3 states, 3 invokes for a full cycle). Reaching a target takes at most 2 invokes.
- Both changed within the 1.2–1.5 s the probe waited. Toggling shuffle reorders the whole queue (a different set of upcoming songs shows) and switching it off restores the original order exactly, so the next track does follow the mode.
- **Events: none.** Changes made in the app raised no `PlaybackInfoChanged` (0 events for 2 shuffle toggles and 3 repeat invokes). The 2 `MediaPropertiesChanged` seen at the third repeat invoke were most likely a track change at that moment and are not caused by the button. So changes made in the app are not pushed: the agent must read the two buttons itself. A UI Automation property-changed handler on the buttons was **not tried** (UNVERIFIED).
- Reading cost: a whole-window element read is 40–360 ms; a read scoped to the two buttons was not timed (UNVERIFIED, expected well under that).
- Language: the repeat state is read from the Name, which is English here. In another UI language the names differ (UNVERIFIED); the cycle order is the reliable part.

## B. Queue (Up Next)
**Where it is.** The right-hand pane: the right-most of two `SplitViewPane` elements that both have the id `PaneRoot` (the other is the left navigation sidebar; pick the one that contains the "Playing Next" toggle). Header toggles: `Playing Next` (toggle, `1` = shown) and `History` (toggle, `0`; **not probed**).
- **Open or closed:** the pane exists only while open. The toolbar button `PlayQueueToggleButton` (class `ToggleButton`, `TogglePattern`, `1` = open) closes and opens it; with the pane closed no rows are readable. After opening, rows were readable within the first ~1 s poll. The toggles worked by pattern only.
- **Two lists:** `PlayQueueListView` ("Playing Next", the user's queue; has a `Clear` button, not touched) and `PlayQueueListViewAutoplay` ("AutoPlay", suggested songs after the queue; has an `AutoPlay` toggle, not probed). With a single song playing only the autoplay list exists.
- **The current song is not in the lists.** Read it from SMTC. There is no "current row" marker.

**Rows.** Class `ListViewItem` (patterns: `SelectionItem`, `ScrollItem`), no AutomationId, no `InvokePattern`. Name: `"<title> <artist> — <album> <duration spoken>"` (for example `"… 4 minutes, 49 seconds"`; the duration text can be cut off with "…" on long names). Three child `TextBlock`s give the clean fields: title, `"<artist> — <album>"` (artist may list several names), and `"m:ss"`. A row's `SelectionItem.IsSelected` is `true` after it was selected (selecting changes nothing else).

**How many rows.** The pane exposes **at most about 20 rows in total** (queue + autoplay): across 6 different queues the totals were 20, 20, 20, 20, 18 and 11, never more. The scroll extent is the same for every long queue, scrolling to the very end (by `ScrollPattern` and by `ScrollItemPattern`) never produced a 21st row, and after skipping 5 tracks the list was still 20 rows (the window slides). Rows below the visible area are exposed with `IsOffscreen = true`, so a read does not need to scroll. It was **not** possible to test a queue larger than the cap or one with several hundred songs, so the real size of a long queue is unknown to the agent: report at most the next 20.
- Read cost: 60–300 ms for a full window of rows (measured on several reads; the spread was not explained).

**Play a row.**
- **Directly by pattern: NO.** On a visible row: `SelectionItemPattern.Select`, `LegacyIAccessible.DoDefaultAction` (the row's default action is `"Double Click"`), the same with `SetFocus` or `Select` first, and MSAA `Select(take focus + selection)` were all accepted (`Ok`) and none started the song. The row's inner container has no default action. Rows have no buttons (no "More" or "Play" element, even when selected). (The earlier "Select then Enter plays it" needed a key press, which is not allowed.)
- **By skipping: YES.** Row *n* (0-based, in "Playing Next") is *n + 1* presses of SMTC `Next` away. Sent back to back (100 and 300 ms apart) it landed on the right song both times, in **0.8–1.3 s**, and the rest of the queue was unchanged (the first row afterwards was the one after the target). Intermediate songs briefly become the current track (visible in the titles read after each skip), so there is a short audible blip of each skipped song. Skipping into the autoplay part (counting the whole "Playing Next" list first) and going backwards through History with `Previous` were not tried.

**Stable id.** None. A row has no AutomationId, the name can repeat, and the index changes every time the song advances. Identify a row by (index, title) and check the title again immediately before skipping; if it no longer matches, refuse instead of guessing.

**Change detection.** Polling is not needed. A UI Automation `StructureChanged` handler on the queue pane (subtree) gave **0 events in 2 s idle and 113 events for one skip, the first about 0.9 s after the skip**. So: subscribe on the `uia` thread while the pane is open, debounce (300–500 ms), re-read. SMTC `MediaPropertiesChanged` (fires on every track change) is the second signal and also works when the pane is closed. Whether a change that does not move the song (adding or removing a row by hand, "Play Next") raises the same event was **not tried** (UNVERIFIED).

**Shuffle and the queue.** Shuffle on reorders the visible window (different songs, same count); off restores the original order (checked). While shuffled, rows are the shuffled order.

## C. App search
**Search box.** The sidebar button `Search_Button` (`Invoke`) expands the sidebar and shows an `AutoSuggestBox` containing a `TextBox` (UIA Edit, AutomationId `TextBox`, name "Search") that supports `ValuePattern`. **`SetValue` works with no typing.** The box keeps the previous query; `SetValue` replaces it.
- **How the search starts:** after `SetValue`, either (A) a suggestions popup (`SuggestionsList`, items support `Invoke`) appears after about 1.0–1.4 s, whose first item has exactly the query as its name: invoking it runs the full search; or (B), when the results page is already open, the page updates by itself (status text changes, no popup). The agent must handle both.
- **Timing:** open the box, set the text, invoke the plain-query suggestion, results page with its first items: about **3.9 s** in total (two runs: 3871 and 3910 ms). The live path (B) was not timed.
- **Fragile detail (seen once):** a `SetValue` made 0.2 s after the box appeared did nothing visible (no popup, no new results), although the text was in the box. Setting it again a moment later worked. Wait about a second, or set the text again if nothing happened within ~2 s.
- **Suggestion kinds** in the popup (names): the query itself, query completions, `"<name> Artist"`, `"<title> Song · <artist>"`, `"<title> Playlist · <curator>"`. Invoking a *song suggestion* only navigates to that song's single/album page (nothing plays).

**Catalog or library.** Two toggle buttons on the results page, `TogglePattern`: `SearchAppleMusic` ("Apple Music") and `SearchLibrary` ("Your Library"); exactly one is on (`1`). Switching re-filters the results without a new search. The status text `SearchPageStatus` says `Showing results for “<query>”`.

**Result page.** A list of sections; each section is a wide `ListViewItem` named after its heading, holding a `GridView` of items (`GridViewItem`, `Invoke`; some also `SelectionItem`). Only sections near the viewport exist in the tree: scroll the page by `ScrollPattern.SetScrollPercent` on the page `ListView` (the one about 1100 px wide and the window's height) to realize the next ones; `ScrollItemPattern` on a section also works.
- **Catalog sections seen, in order:** Top Results, Artists, Albums, Songs, Playlists, Music Videos, Profiles. Top Results holds mixed cards whose names carry the kind: `"<title> Song · <artists>"`, `"<name> Artist"`, `"<title> Album · …"`, `"<title> Playlist · <curator>"`. Artists: `"<name>"`. Albums: `"<title> <artist>"` (Singles end in " - Single"). Songs: `"<title> <artist>"`. Playlists: `"<title> <curator>"`. Items per section: 6–17.
- **Library sections seen:** Artists, Albums, Songs, Composers (Playlists when the library has a match; not seen). A library song row is named like an album-page row: `"Track <n> <title> <duration spoken>"` (no artist in the name).
- **What an item exposes:** only its name (and the text children). **No link, no id and no artwork** is reachable: `HelpText`, `ItemType`, `ItemStatus`, `AriaProperties`, legacy `Value`/`Description` are all empty; the only legacy default action is `"Double Click"`. So the app search cannot produce the contract's `SearchResult.url` or `artwork_url`.

**Play a result.** `Invoke` on the item:
| Result | What `Invoke` does | To play it |
|---|---|---|
| **Library song** | **plays it at once** (title changed and status became Playing within about 0.25 s; verified) | nothing more |
| Catalog song | navigates to the song's single/album page (~0.2 s), nothing plays (verified, status stayed paused) | the MVP 3 route on that page (row `"Track n …"`, its menu `Play “title”`); on a Single's page `PlayButtonElement` plays it (verified by restoring a single this way) |
| Catalog album | navigates to the album page (~0.2 s) | `PlayButtonElement` on the page: **verified**, it started the album from its first track (~0.5 s) |
| Catalog playlist, artist, library album and the other kinds | **not tried** (UNVERIFIED); expected to navigate like the catalog album | album pages and library playlist pages have `PlayButtonElement` (MVP 3 and this spike); a *catalog* playlist page's Play button was not tried |

**Side effects.** The window leaves the user's page (search results, then the song/album page), the sidebar expands while the search box is open, and the search box keeps the new query. **Back works:** one `Invoke` on `NavigationViewBackButton` returned from a song page and from an album page to the results page (verified twice; the query and the scope were still there the first time). Nothing restores the page the user was on before the search; the agent can only go back as many times as it navigated.

## Proposed contract for C-006 (from what was observed only)
Only what the probes showed can be promised. Open choices are marked **(root decides)**.

1. **Shuffle and repeat** (A: UIA)
   - `GET /player/modes` → `{ "shuffle": true, "repeat": "off|all|one" }`; `POST /player/modes` `{ "shuffle"?: bool, "repeat"?: "off|all|one" }` → the same body (the state after the change). `204` is not enough: the app may refuse, so return the state read back.
   - WebSocket event `modes` → `{ shuffle, repeat }`, sent on connect (after `volume`) and when the agent sees a change. The app gives no event, so the agent has to read the buttons (polling interval **(root decides)**, or a UI Automation property-changed handler, which still has to be tried); say that a change made on the laptop can reach the phone with a delay.
   - Errors: `409 APPLE_MUSIC_NOT_RUNNING`, `501 NOT_SUPPORTED` when the buttons are not found (UI Automation off, other window mode, other app version), `500 INTERNAL` if the app refused or the state did not change.
2. **Queue** (B: read GO, play PARTIAL)
   - `GET /queue` → `{ "items": [ { "index": 0, "title", "artist", "album", "duration_ms", "source": "queue|autoplay" } ] }`, at most 20 items, the next songs in order (the current song is not included; it is in `PlayerState`). `artist` and `album` split from `"<artist> — <album>"`; `duration_ms` from `"m:ss"`. State plainly that this is **only what Apple Music shows (about the next 20)**, not the whole queue.
   - `POST /queue/play` `{ "index": 3, "title": "…" }` → `204` once that song is current (about 1–1.5 s). `title` is required and must still match the row, so a queue that moved on is not skipped into the wrong song: otherwise `404 NOT_FOUND`. `400` for a bad index. Say that songs in between start for a moment.
   - WebSocket event `queue` → `{ items }`, pushed when the queue pane changes (debounced), plus on connect (after `modes`). Needs the pane to be open; the agent opens it when a client asks for the queue and restores the pane afterwards **(root decides: always open it, or only if the user already had it open)**. When it cannot read the queue the answer is `501`.
   - Errors: `409 APPLE_MUSIC_NOT_RUNNING`, `501 NOT_SUPPORTED`.
3. **Search through the app** (C: library GO, catalog PARTIAL)
   - The existing `GET /search` (iTunes catalog, with `url` and `artwork_url`) stays. Add a second, **separate** endpoint because the results have a different shape (no link, no id, no artwork): `GET /app-search?q=&scope=catalog|library` → `{ "results": [ { "section": "Songs", "index": 0, "kind": "song|album|artist|playlist|…", "title", "subtitle" } ] }`. Sections as the app shows them; at most the items the app exposes (about 6–17 per section). `scope=library` is the way to find the user's own music.
   - `POST /app-search/play` `{ "q", "scope", "section", "index", "title" }` → `204` after playback started. The agent repeats the search (the app page cannot be held between calls), finds that item by section, index and title (`404` if it changed), and: library song → invoke; catalog album → invoke, then the page's Play button; catalog song → invoke, then the MVP 3 row route or the Single's Play button. Catalog playlists and the other kinds were not tried, so a first version can answer `501` for them. `409`, `501`, `404`; a long call like `/play/track` (allow the same 45 s).
   - State the side effect: the Apple Music window moves to the search and result pages and is not moved back. Latency: a search is about 4 s, a play 1–5 s.
4. **Notes for the rest of the contract:** nothing here needs a new top-level error code; `404`, `409`, `501` cover it. Sizes and the 20-row cap should be written into the contract text.

## Fragility and how to detect breakage
- Everything rests on AutomationIds and class names that Apple can rename in any update: `ShuffleButton`, `RepeatButton`, `PlayQueueToggleButton`, `PaneRoot` (twice!), `PlayQueueListView`, `PlayQueueListViewAutoplay`, `Search_Button`, `TextBox`, `SuggestionsList`, `SearchAppleMusic`, `SearchLibrary`, `SearchPageStatus`, `NavigationViewBackButton`, `PlayButtonElement`. Before each use check that the needed element exists; if not, answer `501 NOT_SUPPORTED` with a clear message (never guess another element). The repeat names, section headings and "Song ·" kind labels are English text.
- Row and result text depends on the language, the user's library and the account (the song names in the page can differ from the catalog's); match by section and position and verify the title.
- The queue cap (about 20 rows) and the section list are app behaviour, not a contract: a future update can change both.
- Not tried at all, so unsupported until tried: minimized window (the agent already restores it for MVP 3), the mini player, full screen / "big screen" mode (where volume showed two sliders earlier), a narrower window where the queue pane may not fit, other UI languages, and the `History` list.
- Re-run the probes (`cargo run --example spike_mvp4_find -- <text>` first) after an Apple Music update. `spike_mvp4_queue -- summary` and `spike_mvp4_search -- flow <query>` are the quickest regression checks; `spike_mvp4_uia_shuffle` toggles shuffle and repeat and restores them.

## Manual steps still needed
1. On the laptop, try the other window modes (mini player, full screen, narrow window, minimized) and note whether the shuffle/repeat buttons, the queue pane and the search box still exist. This decides the `501` cases.
2. Check that `History` can be read the same way (toggle `History`, read the rows) if a "previous songs" list is wanted.
3. Build a queue by hand with "Play Next" / "Play Later" and check that the pane changes raise the structure event and that the rows appear in order; check what happens with a queue that is longer than the 20-row window when songs are added.
4. Try a UI Automation property-changed handler on `ShuffleButton` and `RepeatButton` (would replace polling).
5. Press Play on a *catalog playlist* page reached from a search result.
6. Run the search flow with a non-English UI language if one is ever used.
