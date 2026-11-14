# MVP 3 — Playlists & search (android-controller)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
A bottom navigation bar (Player · Playlists · Search) for playing saved playlists and searching the catalog.

## Gate
- MVP 2 is ticked, or its leftovers are first on the todo list.
- Not a blocker: the agent may answer `501 NOT_SUPPORTED` for playlists. The app must handle that well.

## Build
- [ ] `core/models/`: `Playlist { id, name }`, `SearchResult { id, title, artist, album, artwork_url, url }`.
- [ ] `api_client`: `getPlaylists()`, `playPlaylist(id)`, `search(q, {limit = 25})`, `playTrack(url)`.
- [ ] `app_router.dart`: bottom navigation with Player, Playlists and Search, keeping each tab's state.
- [ ] Playlists screen: list, pull-to-refresh, tap to play (then switch to the Player tab). `NOT_SUPPORTED` → "Playlists aren't available on this laptop yet", not a generic error.
- [ ] Search screen: text field debounced 300 ms, at least 2 characters, results with artwork thumbnails (HTTPS only), tap → `playTrack(url)` → Player tab. Show empty, loading and error states.
- [ ] Optional: show the agent's `playlist_name` on the now-playing card when it isn't null.

## Tests (required)
- Round-trip `playlists_response`, `play_playlist_request`, `search_response` and `play_track_request`.
- Search debounce with fake time: typing "ab" quickly → 1 request; "a" → no request.
- Playlists: 501 → the explanation state is shown.
- Tapping a search result calls `playTrack` with that result's `url`.

## Done when
- Step 3 is green.
- With the agent's MVP 3, searching and playing a result, and playing a playlist, work on the phone (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-3.md)
1. Search tab: type an artist → results with artwork after a short pause. Tap one → it plays on the laptop.
2. Playlists tab: your playlists are listed; tap one → it plays.
3. If the agent can't read playlists → the friendly "not available yet" message, and no crash.
4. Switch tabs → the search text and results are kept.
5. Turn the phone offline → search shows an error state; back online → it works.

## Next
Roadmap done. Run every `docs/smoke-tests/` checklist, then `/contract-check` in the root window.
