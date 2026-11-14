# desktop-agent — Rust Windows agent

Background `.exe` that controls the Apple Music app on Windows and exposes the LAN API.
**Read `../CLAUDE.md` for the roadmap and the API contract — that contract is the source of truth.**

## Stack
- Rust stable, edition 2021, target `x86_64-pc-windows-msvc`
- `tokio` (multi-thread) + `axum` (HTTP + WebSocket)
- `windows` crate (windows-rs), features:
  `Media_Control`, `Foundation`, `Foundation_Collections`, `Storage_Streams`,
  `Win32_Foundation`, `Win32_Media_Audio`, `Win32_System_Com`, `Win32_System_Threading`
- `serde` / `serde_json`, `tracing` + `tracing-subscriber`
- `mdns-sd` (discovery), `tray-icon` + `winit` or `tao` (tray), `rand` + `base64` (tokens)
- `reqwest` (MVP 3: iTunes Search API), `uiautomation` (MVP 2/3 spikes only)

## Structure
```
src/
  main.rs            // startup: config, tray, server, mdns
  config.rs          // port, tokens, autostart; stored in %APPDATA%\am-remote\config.json
  api/
    mod.rs           // router, auth middleware, error type → JSON error body
    player.rs  volume.rs  library.rs  ws.rs  pair.rs
  media/
    mod.rs           // trait MediaBackend
    smtc.rs          // SMTC impl (play/pause/next/prev/seek/metadata/artwork)
    audio.rs         // Core Audio per-app volume
    uia.rs           // UI Automation of the Apple Music window (playlists, play a song/playlist) — isolated, feature `uia`
    library.rs links.rs state.rs tracker.rs worker.rs   // pure matching rules, link validation, player state, change detection, media thread
  search.rs          // iTunes Search API client
  discovery.rs       // mDNS advertise
  tray.rs
```

## Key implementation notes
- **Finding Apple Music in SMTC:** iterate `GetSessions()`, match `SourceAppUserModelId()` containing `AppleMusic` (verify the exact AUMID on the real machine and put it in a const). Fall back to "not running" — never control another app's session.
- **Blocking WinRT calls:** `IAsyncOperation::get()` blocks. Wrap in `tokio::task::spawn_blocking`, or run a dedicated media thread that owns all WinRT/COM objects and receives commands over a channel (preferred — avoids COM apartment issues).
- **COM init:** call `CoInitializeEx(COINIT_MULTITHREADED)` once on the media thread before Core Audio calls.
- **Volume:** `IMMDeviceEnumerator` → default render device → `IAudioSessionManager2` → enumerate sessions → match every session whose process image path is in the Apple Music package folder (`AppleInc.AppleMusicWin_*`) → `ISimpleAudioVolume`. Map 0.0–1.0 ↔ 0–100. **Verified 2026-11-14:** the audible session belongs to `AMPLibraryAgent.exe`, while `AppleMusic.exe`'s own session stays inactive (the mixer shows both as "Apple Music"; only the active one changes loudness). So set all package sessions together and read the active one (`media/audio.rs`). This is the Windows per-app volume; Apple Music's in-app slider is a separate control and is not linked to it.
- **Live updates:** subscribe to `MediaPropertiesChanged`, `PlaybackInfoChanged`, `TimelinePropertiesChanged` on the session and `SessionsChanged` on the manager; push into a `tokio::sync::broadcast` channel that `ws.rs` fans out. Re-subscribe when the session changes (app restarted).
- **Position:** SMTC timeline updates are sparse; compute live position as `position + (now - LastUpdatedTime)` while playing.
- **Artwork:** `props.Thumbnail()` → `OpenReadAsync` → read stream to bytes; cache it and bump `artwork_rev` when the track changes.
- **UI Automation is fragile:** keep it behind `media::uia`, behind a cargo feature `uia`, and return `NOT_SUPPORTED` when it fails instead of crashing.
- **Playlists and songs (MVP 3), verified 2026-11-14 — see `docs/spikes/link-playback.md`:** links never autoplay; `music://` links only navigate. Playlists: sidebar `NavigationViewItem`s whose AutomationId ends `IKIND:ePlaylist` (that id is the contract id); select it, wait for the page header `ContainerDetailHeaderLockupRegularContent` to carry the playlist name, invoke `PlayButtonElement`. A song: open its `music://` link, find the page row `"Track <n> <title> <duration>"` (not the queue pane's rows), invoke the button inside the row, then the menu entry `Play “<title>”`. Never simulate keys or the mouse. Selecting the sidebar entry "Playlists" **toggles** the list.
- **UI Automation speed:** never ask UI Automation to list or search the desktop's windows (about 10 s: it queries every app's provider). Find Apple Music's windows with Win32 `EnumWindows` and ask UI Automation only about them; scope searches to a small container.
- **`uia` feature:** on by default. `cargo build --no-default-features` leaves it out; `/playlists`, `/play/playlist` and `/play/track` then answer `501 NOT_SUPPORTED`. Run the checks in both modes.
- **Probes:** `examples/` holds the programs that found these facts (`library_check`, `spike_*`, `uia_*`, `audio_sessions_detail`, …). Re-run them when an Apple Music update breaks something.

## Conventions
- No `unwrap()`/`expect()` outside `main` startup; use `anyhow` in bins, a typed `ApiError` in handlers.
- All handlers return the error JSON shape from the contract.
- Log with `tracing`; never log tokens.
- Release build: `#![windows_subsystem = "windows"]` (no console window); keep console in debug.
- Bind to `0.0.0.0:8765` by default; port configurable.

## Commands
```
cargo run                       # debug with console logs
cargo build --release           # target\release\am-remote-agent.exe
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --no-default-features   # without UI Automation (the `uia` feature)
```

## Current status
- Contract changes applied: C-005 (see ../contract/CHANGELOG.md; update via /sync-contract)
- [x] MVP 0 — server skeleton, tray, pairing, mDNS
- [x] MVP 1 — SMTC controls, Core Audio volume
- [x] MVP 2 — metadata, artwork, WebSocket push; UIA queue spike
- [x] MVP 3 — link-playback spike, playlists, search, play track
