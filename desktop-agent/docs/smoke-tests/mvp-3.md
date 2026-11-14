# MVP 3 smoke test (desktop-agent)

Needs a human with Apple Music open and signed in. Playing starts music and changes Apple Music's queue and window.
How it works and what was verified on the dev laptop (2026-11-14, Apple Music 1.1792.39942.0, English UI): `docs/spikes/link-playback.md`.

Before you start: quit any running agent (tray, Quit), `cargo run`, and pair as in `mvp-0.md`:
```powershell
$base = "http://localhost:8765/api/v1"
$r = Invoke-RestMethod -Method Post "$base/pair" -ContentType 'application/json' -Body '{"code":"<code from tray>","device_name":"pc-test"}'
$h = @{ Authorization = "Bearer $($r.token)" }
```

1. Search.
   ```powershell
   $found = Invoke-RestMethod "$base/search?q=tulus&limit=5" -Headers $h
   $found | Format-Table title, artist, album
   ```
   Expected: up to 5 songs, each with `artwork_url` starting with `https://` and `url` starting with `https://music.apple.com/`. Without `limit` you get up to 25.
   Errors: `?q=a` gives `400`, `?q=tulus&limit=51` gives `400`.
2. Play a search result.
   ```powershell
   Invoke-RestMethod -Method Post "$base/play/track" -Headers $h -ContentType 'application/json' -Body (@{ url = $found[0].url } | ConvertTo-Json)
   Invoke-RestMethod "$base/player/state" -Headers $h
   ```
   Expected: no output from the POST (204) after 1-4 s, Apple Music plays that song (its page opens in the app), and `title` in the state is the song. The answer only comes once the song really plays.
   Try a second and a third result too, and one from an album with many songs (the agent scrolls the page to find the row).
3. Playlists.
   ```powershell
   $lists = Invoke-RestMethod "$base/playlists" -Headers $h
   $lists | Format-Table name, id
   Invoke-RestMethod -Method Post "$base/play/playlist" -Headers $h -ContentType 'application/json' -Body (@{ id = $lists[1].id } | ConvertTo-Json)
   ```
   Expected: your library playlists with their names, in about a second; then the chosen playlist starts. Call `GET /playlists` several times in a row: it must give the same list every time.
4. **Stable ids (not verified yet):** write down one playlist id, quit Apple Music completely, start it again and play something, then call `GET /playlists` and use the old id with `POST /play/playlist`. Expected: the same id works. If it gives `404`, tell me: the ids are not stable across restarts and the contract needs another approach.
5. Errors stay clean:
   - `POST /play/track` with `{"url":"https://evil.com/id/album/x/1?i=2"}` gives `400`.
   - `POST /play/track` with an album link (no `?i=`) gives `400`.
   - `POST /play/playlist` with `{"id":"nope"}` gives `404`.
   - Close Apple Music: `POST /play/track` and `GET /playlists` give `409 APPLE_MUSIC_NOT_RUNNING`.
6. Apple Music in different states. Try playing a song or a playlist when Apple Music is: minimized (it should be restored), showing the mini player only, with the Queue pane open or closed, in full screen. Expected: it works, or you get `501 NOT_SUPPORTED` with a message saying which step failed. It must never hang: every call returns within about 35 s.
7. No internet: turn Wi-Fi off on the laptop.
   Expected: `GET /search` gives `500` with a message about the internet connection and the agent keeps running; turn Wi-Fi on and it works again. Playing a playlist still works (it does not need the internet from the agent).
8. Apple Music update (or any change of its sidebar): the playlist and play endpoints answer `501 NOT_SUPPORTED` instead of hanging. You cannot easily test this: if an update ever breaks them, run `cargo run --example library_check -- list` and send me the output.
9. Optional: `cargo run --no-default-features` builds the agent without UI Automation: `GET /playlists`, `POST /play/playlist` and `POST /play/track` answer `501 NOT_SUPPORTED`, everything else works.
10. Optional: if the songs you search for are in a different country's catalog than the one the agent picks (it uses your Windows region), add `"storefront": "id"` (two letters) to `%APPDATA%\am-remote\config.json` and restart.

Needs you: all of it, especially step 4 (ids across a restart) and step 6 (other window states).
