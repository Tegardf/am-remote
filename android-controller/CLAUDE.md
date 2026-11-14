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
- On `NO_AUDIO_SESSION` (only `/volume` answers it: Apple Music is there, nothing has played yet): switch off only the volume slider and mute button, with a short note. Keep the playback buttons and now-playing view, and do not show the "Open Apple Music" empty state. The last volume level is stale when `apple_music` turns false or a volume call answers `409`: read `GET /volume` again when Apple Music runs again, when playback starts, and after a socket reconnect.
- On `401`: clear token, go back to pairing.
- On Wi-Fi loss: show offline banner, reconnect automatically; re-run discovery if the saved host fails (after 3 failed checks in a row). The TXT `ip` can be out of date after a DHCP renewal or a Wi-Fi switch, so when following the laptop the app tries `ip` first, then the other IPv4 addresses the resolver gave, and moves to the first one that answers `GET /status`; if none does, it stays and tries again next round.
- Search box: debounce 300 ms, min 2 chars.
- Hardware volume keys control laptop volume while the Player screen is in front (MVP 1 nice-to-have).

## Android config
- `AndroidManifest.xml`: `INTERNET`, `ACCESS_NETWORK_STATE`, `CHANGE_WIFI_MULTICAST_STATE`; `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_MEDIA_PLAYBACK` for `audio_service`, `POST_NOTIFICATIONS` (Android 13+, so the media notification can show) and `WAKE_LOCK` (`audio_service` takes it when the session starts playing; without it the notification is never posted, as seen on the emulator).
- Cleartext HTTP to the LAN is required: add `network_security_config.xml` allowing cleartext (LAN only: plain HTTP goes only to the paired agent. The one other kind of traffic is search artwork, which is loaded directly over HTTPS).

## Commands
```
flutter pub get
dart run build_runner build --delete-conflicting-outputs
flutter run
flutter build apk --release
flutter analyze && flutter test
```

## Current status
- Contract changes applied: C-005 (see ../contract/CHANGELOG.md; update via /sync-contract)
- [x] MVP 0 — connect screen, discovery, manual IP, pairing, connection indicator
- [x] MVP 1 — play/pause/next/prev, volume slider, mute
- [x] MVP 2 — now-playing card, artwork, progress/seek, media notification
- [x] MVP 3 — playlists screen, search screen, bottom nav
