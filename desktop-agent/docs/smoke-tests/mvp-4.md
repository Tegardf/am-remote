# MVP 4 smoke test: shuffle/repeat, queue, app search (needs the real laptop)

Needs Apple Music open with its **main window visible** (not minimized), some music queued, and the agent running (`cargo run`). The calls below change what plays and what Apple Music shows: do them when that is fine. Replace `$token` with a paired device's token (from the phone's pairing, never copy it anywhere) and use `curl.exe` in PowerShell:

```powershell
$h = @{ Authorization = "Bearer $token" }
$b = "http://localhost:8765/api/v1"
```

Already checked on the real app while building (2026-11-15, Apple Music 1.1792.39942.0, English UI): modes read and set (about 0.1-0.4 s), the queue read (about 0.1 s), the connect burst and the `modes` push (about 1.4 s after a change), app search read in both scopes (about 3-9 s), and every `404`/`501`/`400` answer. **Not run against the real app, so they are the first things to try:** playing a queue row, playing a search result.

1. **Shuffle and repeat from the API.**
   `Invoke-RestMethod -Method Post -Headers $h -ContentType application/json -Body '{"shuffle":true,"repeat":"all"}' $b/player/modes`
   Expected: the answer is `{ shuffle: true, repeat: "all" }` and Apple Music's two buttons show it. Then `repeat":"one"` (one press more), then set both back off.
2. **From the laptop to the phone.** Open a WebSocket (`npx --yes wscat -c "ws://localhost:8765/api/v1/ws?token=$token"`: the first four lines are `state`, `apple_music`, `volume`, `modes`). Press shuffle in Apple Music itself.
   Expected: a `modes` line within about 2-3 s. Press repeat: another `modes` line. Pressing nothing sends nothing.
3. **Queue with the pane closed.** Close the queue pane in Apple Music, then `Invoke-RestMethod -Headers $h $b/queue`.
   Expected: the pane opens by itself (and stays open), up to 20 `items`, `queue` rows first and then `autoplay` rows, `index` counting across both, none of them the song that is playing now.
4. **Play a queue row.** Take row 2 from step 3 and call `POST /queue/play` with its `index` and exact `title`.
   Expected: `204` after about 1.5-2.5 s, that song is playing, and the rest of the queue is unchanged (a second `GET /queue` starts with the row after it). The songs in between start for a moment (a short blip).
   - The same call with a wrong `title` -> `404`, nothing skips.
   - **Try one `autoplay` row** (an item with `source: autoplay`) and note the result: `204` and the song plays, or `501` "Skipping into the autoplay songs did not reach this song". This path was not tried while building; if it never works, `/queue/play` for autoplay rows can answer `501` by design.
5. **Search the library and play a song.** `GET $b/app-search?q=<an artist you have in your library>&scope=library`, then `POST /app-search/play` with the first `Songs` result (`q`, `scope`, `section`, `index`, `title` as returned).
   Expected: the search takes 3-5 s, the song plays at once (the play call returns `204` within a few seconds). Results have no artwork or link.
6. **Search the catalog and play an album.** `GET $b/app-search?q=<an artist>&scope=catalog` (the first catalog search can take about 9 s), play a result of the `Albums` section.
   Expected: the album page opens and the album starts from its first track within about 5-10 s (`204`). Also try a catalog **song** from `Songs`: it opens the song's page and plays that song (a Single's Play button, or the row menu on an album page). An `Artists`, `Playlists` or `Music Videos` result answers `501`.
7. **Window states.**
   - Close the queue pane and call `GET /queue`: it reopens it.
   - Minimize Apple Music and call `GET /player/modes`, `GET /queue`, `GET /app-search`: the agent restores the window for these (as it does for playlists). Note what happens, and that the `modes` event of an open WebSocket stays quiet while the window is minimized (it never restores the window by itself).
   - Mini player or full screen: note which of the six calls still work and which answer `501`.
8. **A broken selector answers `501`, no hang.** In a debug build, change the id in `SHUFFLE_BUTTON_ID` (`src/media/modes.rs`) to something wrong, run again and call `GET /player/modes`.
   Expected: `501` with a message, within a second or two; the agent stays up. Put the id back.
9. **Concurrent calls.** Start a slow call (`GET /app-search?q=...&scope=catalog`) and, while it runs, `GET /player/modes` in a second window.
   Expected: the second answer comes after the first finishes (calls that drive the window run one at a time), both are correct, nothing hangs.

Things to note and report: the real durations of steps 4-6, whether the autoplay row worked, and which window modes break which endpoint.
