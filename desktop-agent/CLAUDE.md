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
    uia.rs           // UI Automation (playlists/queue) — isolated, feature-gated
  search.rs          // iTunes Search API client
  discovery.rs       // mDNS advertise
  tray.rs
```

## Key implementation notes
- **Finding Apple Music in SMTC:** iterate `GetSessions()`, match `SourceAppUserModelId()` containing `AppleMusic` (verify the exact AUMID on the real machine and put it in a const). Fall back to "not running" — never control another app's session.
- **Blocking WinRT calls:** `IAsyncOperation::get()` blocks. Wrap in `tokio::task::spawn_blocking`, or run a dedicated media thread that owns all WinRT/COM objects and receives commands over a channel (preferred — avoids COM apartment issues).
- **COM init:** call `CoInitializeEx(COINIT_MULTITHREADED)` once on the media thread before Core Audio calls.
- **Volume:** `IMMDeviceEnumerator` → default render device → `IAudioSessionManager2` → enumerate sessions → match process ID of `AppleMusic.exe` (verify name) → `ISimpleAudioVolume`. Map 0.0–1.0 ↔ 0–100.
- **Live updates:** subscribe to `MediaPropertiesChanged`, `PlaybackInfoChanged`, `TimelinePropertiesChanged` on the session and `SessionsChanged` on the manager; push into a `tokio::sync::broadcast` channel that `ws.rs` fans out. Re-subscribe when the session changes (app restarted).
- **Position:** SMTC timeline updates are sparse; compute live position as `position + (now - LastUpdatedTime)` while playing.
- **Artwork:** `props.Thumbnail()` → `OpenReadAsync` → read stream to bytes; cache it and bump `artwork_rev` when the track changes.
- **UI Automation is fragile:** keep it behind `media::uia`, behind a cargo feature `uia`, and return `NOT_SUPPORTED` when it fails instead of crashing.

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
cargo clippy -- -D warnings
cargo test
```

## Current status
- Contract changes applied: C-000 (see ../contract/CHANGELOG.md; update via /sync-contract)
- [ ] MVP 0 — server skeleton, tray, pairing, mDNS
- [ ] MVP 1 — SMTC controls, Core Audio volume
- [ ] MVP 2 — metadata, artwork, WebSocket push; UIA queue spike
- [ ] MVP 3 — link-playback spike, playlists, search, play track
