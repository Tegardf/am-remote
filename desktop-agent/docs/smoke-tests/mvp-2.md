# MVP 2 smoke test (desktop-agent)

Needs a human with Apple Music playing. Real-machine facts, verified on the dev laptop (2026-11-14):
- Apple Music puts `"Artist — Album"` in SMTC's artist field; the agent splits it into `artist` and `album`.
- The cover is a real JPEG (about 100-300 KB).
- SMTC says seeking is **disabled** for Apple Music (`IsPlaybackPositionEnabled = false`). The seek call is accepted, but whether the audio really jumps is unknown: step 4 finds out.

Before you start: quit any running agent (tray, Quit), then `cargo run` again. Pair as in `mvp-0.md` and keep `$h` and `$base`:
```powershell
$base = "http://localhost:8765/api/v1"
$r = Invoke-RestMethod -Method Post "$base/pair" -ContentType 'application/json' -Body '{"code":"<code from tray>","device_name":"pc-test"}'
$h = @{ Authorization = "Bearer $($r.token)" }
$token = $r.token
```

1. State.
   ```powershell
   Invoke-RestMethod "$base/player/state" -Headers $h
   ```
   Expected: `status` `playing`, the right `title`, `artist` and `album` (separate), `duration_ms`, a `position_ms` that is about where the song is now, `artwork_rev` 1 or more, `playlist_name` empty (null).
2. Artwork.
   ```powershell
   Invoke-WebRequest "$base/player/artwork" -Headers $h -OutFile "$env:TEMP\cover.jpg"; Start-Process "$env:TEMP\cover.jpg"
   ```
   Expected: the cover of the current song opens.
3. WebSocket (needs Node, which you have; `ws` is fetched by npx and nothing is installed globally):
   ```powershell
   npx --yes wscat -c "ws://localhost:8765/api/v1/ws?token=$token"
   ```
   Expected: three lines at once, in this order: `{"type":"state",...}`, `{"type":"apple_music","data":{"running":true}}` and `{"type":"volume",...}` (no `volume` line if nothing has played since Apple Music started). Type `{"type":"ping"}` and press Enter: `{"type":"pong"}` comes back.
   Keep it open for the next steps.
4. Seek (in another PowerShell window, with `$base`/`$h` set again):
   ```powershell
   Invoke-RestMethod -Method Post "$base/player/seek" -Headers $h -ContentType 'application/json' -Body '{"position_ms":60000}'
   ```
   Expected: Apple Music jumps to 1:00 and wscat shows a new `state` with `position_ms` near 60000.
   **If nothing jumps** (or you get `501 NOT_SUPPORTED`), seeking is not possible for Apple Music through Windows: tell me and the contract will say so.
5. Skip a track in Apple Music itself. Expected: within about a second wscat shows a new `state` with the new title and a higher `artwork_rev` (and no burst of three half-filled states). Pause and resume in Apple Music: a `state` each time. While a song just plays, wscat stays quiet.
6. Change Apple Music's volume in the **Windows mixer** (the entry that works) or with `POST /volume`. Expected: a `{"type":"volume",...}` within about a second.
7. Quit Apple Music completely. Expected: `{"type":"apple_music","data":{"running":false}}` and a `state` with an empty title. Start it again and play something: `apple_music` `running:true` and a `state`, without restarting the agent.
8. Revoke the device in the tray (`Paired devices`, `Revoke pc-test`). Expected: wscat disconnects at once.
9. Errors are still JSON: `Invoke-RestMethod "$base/player/seek" -Method Post -Headers $h -ContentType 'application/json' -Body '{"position_ms":-5}'` gives `400` with `BAD_REQUEST` (re-pair first if you revoked in step 8).

Needs you: all of it (it needs your Apple Music, your ears and the tray).
