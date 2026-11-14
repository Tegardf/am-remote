# MVP 1 smoke test (desktop-agent)

Needs a human: this changes what Apple Music plays and how loud it is.
Real-machine facts, verified on the dev laptop (2026-11-14): Apple Music's AUMID is
`AppleInc.AppleMusicWin_nzyj5cx40ttqa!App` and its process is `AppleMusic.exe`.

Before you start: quit any running agent (tray, Quit), then run `cargo run` again so it has the MVP 1 code.

1. Apple Music is open and playing a song, and the agent is running and paired (see `mvp-0.md`, step 3).
   ```powershell
   $base = "http://localhost:8765/api/v1"
   $r = Invoke-RestMethod -Method Post "$base/pair" -ContentType 'application/json' -Body '{"code":"<code from tray>","device_name":"pc-test"}'
   $h = @{ Authorization = "Bearer $($r.token)" }
   Invoke-RestMethod "$base/status" -Headers $h        # apple_music_running: True
   ```
2. Playback.
   ```powershell
   Invoke-RestMethod -Method Post "$base/player/toggle" -Headers $h    # pauses; no output (204)
   Invoke-RestMethod -Method Post "$base/player/toggle" -Headers $h    # resumes
   Invoke-RestMethod -Method Post "$base/player/pause"  -Headers $h
   Invoke-RestMethod -Method Post "$base/player/play"   -Headers $h
   Invoke-RestMethod -Method Post "$base/player/next"   -Headers $h    # track changes
   Invoke-RestMethod -Method Post "$base/player/previous" -Headers $h
   ```
   Expected: each call returns nothing (204) and Apple Music reacts.
3. Volume. (Apple Music has two entries in the Windows mixer; the agent moves both together, and the audible one is the one that matters. Apple Music's own in-app slider is a separate control and does not move.)
   ```powershell
   Invoke-RestMethod "$base/volume" -Headers $h                                            # { level, muted }
   Invoke-RestMethod -Method Post "$base/volume" -Headers $h -ContentType 'application/json' -Body '{"level":20}'
   ```
   Expected: Apple Music gets quieter and BOTH "Apple Music" entries in the Windows volume mixer show 20 %. Other apps and the master volume don't change.
4. Mute.
   ```powershell
   Invoke-RestMethod -Method Post "$base/volume" -Headers $h -ContentType 'application/json' -Body '{"muted":true}'
   Invoke-RestMethod -Method Post "$base/volume" -Headers $h -ContentType 'application/json' -Body '{"muted":false}'
   ```
   Expected: only Apple Music is muted, and its level stays 20.
5. Validation: `-Body '{"level":150}'` gives `400` with `{"error":{"code":"BAD_REQUEST",...}}`. Put the volume back to your normal level afterwards.
6. Close Apple Music completely.
   Expected: `player/toggle` and `GET /volume` return `409 APPLE_MUSIC_NOT_RUNNING`, and `/status` shows `apple_music_running: False`.
7. Open another app that plays sound (for example a YouTube tab) and call `toggle`.
   Expected: it affects Apple Music only (or returns 409 if Apple Music isn't running). The other app is untouched.
8. Optional: restart Apple Music without playing anything, then call `GET /volume`.
   Expected: `409` until it has played once (open question #3; see `docs/contract-requests.md`).

Needs you: steps 2 to 8 (they need your ears and your Apple Music).
