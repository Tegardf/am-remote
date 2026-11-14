# android-controller — Flutter Android remote

Android app that discovers the desktop agent on the LAN and controls Apple Music on the laptop.
**Read `../CLAUDE.md` for the roadmap and the API contract — that contract is the source of truth.**

## Stack
- Flutter stable, Dart 3, Android only (minSdk 24)
- State: `flutter_riverpod`
- HTTP: `dio` · WebSocket: `web_socket_channel`
- Discovery: `nsd` (Android NSD for `_amremote._tcp`) — manual IP entry as fallback
- Storage: `flutter_secure_storage` (token), `shared_preferences` (last host)
- Media notification / lock screen controls: `audio_service` (MVP 2)
- Models: `freezed` + `json_serializable`

## Structure
```
lib/
  main.dart
  core/
    api_client.dart      // dio, base URL, bearer token, maps error JSON → ApiException
    ws_client.dart       // connect, ping every 15 s, auto-reconnect with backoff
    discovery.dart
    models/              // PlayerState, Volume, Playlist, SearchResult (match contract exactly)
  features/
    connect/             // discover / manual IP / pairing code
    player/              // now playing, controls, volume, seek
    playlists/           // MVP 3
    search/              // MVP 3
  app_router.dart        // bottom nav: Player · Playlists · Search
```

## Behaviour rules
- WebSocket `state` events are the source of truth for the UI; buttons send HTTP commands and wait for the push instead of updating UI optimistically (except volume slider, which updates locally while dragging and sends debounced `POST /volume` ~100 ms).
- Re-fetch `/player/artwork` only when `artwork_rev` changes.
- Interpolate the progress bar locally between `state` events while `status == playing`.
- On `APPLE_MUSIC_NOT_RUNNING`: show a "Open Apple Music on your laptop" empty state, keep the socket alive.
- On `401`: clear token, go back to pairing.
- On Wi-Fi loss: show offline banner, reconnect automatically; re-run discovery if the saved host fails.
- Search box: debounce 300 ms, min 2 chars.
- Hardware volume keys control laptop volume while the Player screen is in front (MVP 1 nice-to-have).

## Android config
- `AndroidManifest.xml`: `INTERNET`, `ACCESS_NETWORK_STATE`, `CHANGE_WIFI_MULTICAST_STATE`; `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_MEDIA_PLAYBACK` for `audio_service`.
- Cleartext HTTP to the LAN is required: add `network_security_config.xml` allowing cleartext (LAN only; no internet traffic in this app).

## Commands
```
flutter pub get
dart run build_runner build --delete-conflicting-outputs
flutter run
flutter build apk --release
flutter analyze && flutter test
```

## Current status
- Contract changes applied: C-000 (see ../contract/CHANGELOG.md; update via /sync-contract)
- [ ] MVP 0 — connect screen, discovery, manual IP, pairing, connection indicator
- [ ] MVP 1 — play/pause/next/prev, volume slider, mute
- [ ] MVP 2 — now-playing card, artwork, progress/seek, media notification
- [ ] MVP 3 — playlists screen, search screen, bottom nav
