# MVP 3 — Playlists & search (desktop-agent)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
List and play saved playlists, search the Apple Music catalog, and play a search result.

## Gate
- `docs/spikes/link-playback.md` exists, with a `**Decision:**` that isn't `NEEDS-MANUAL-RUN`. Otherwise stop and suggest `/spike link-playback`.
- MVP 2 is ticked, or its leftovers are first on the todo list.

## Build
Use the approach the spike chose (LINK / MIXED / UIA), and copy its "Recommendation" section into the plan.
- [ ] `search.rs`: iTunes Search API via `reqwest` (`https://itunes.apple.com/search?term=…&entity=song&limit=…`) → `SearchResult { id, title, artist, album, artwork_url, url }`. `url` is the `trackViewUrl`; use the https artwork URL. Default limit 25, max 50.
- [ ] `GET /search?q=&limit=25`: a `q` shorter than 2 characters is a validation error. Network failure → `INTERNAL` with a clear message.
- [ ] `POST /play/track { url }`: accept only `https://music.apple.com/…` URLs (anything else is a validation error), then play it the spike's way.
- [ ] `GET /playlists` and `POST /play/playlist { id }`: through `media/uia.rs` behind the `uia` cargo feature, unless the spike says otherwise. Ids are opaque, stable strings (e.g. derived from the name).
- [ ] Any approach that fails at runtime → `501 NOT_SUPPORTED` with a helpful message. Never crash, and never hang: put timeouts on UIA calls.

## Tests (required)
- Round-trip the samples `playlists_response`, `play_playlist_request`, `search_response` and `play_track_request`.
- iTunes JSON → `SearchResult` mapping, from a saved fixture (no network in tests).
- URL validation: accepts music.apple.com, rejects others.
- With the `uia` feature off: `/playlists` → 501 `NOT_SUPPORTED`.

## Done when
- Step 3 is green, both with and without `--features uia`.
- Searching and playing a result works with the real app (needs a human).

## Smoke test (→ docs/smoke-tests/mvp-3.md)
1. `GET /search?q=<artist>` → up to 25 results with https artwork.
2. `POST /play/track` with the first result's `url` → Apple Music plays that song within ~5 s.
3. `GET /playlists` lists your library playlists. `POST /play/playlist` starts one.
4. Close the Apple Music sidebar or update the app → the playlist endpoints return 501 instead of hanging.
5. Search with Wi-Fi off on the laptop → a clear error, and the agent keeps running.

## Next
Roadmap done. Run every `docs/smoke-tests/` checklist with the phone, then `/contract-check` in the root window.
