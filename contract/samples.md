# Contract samples (v1)

One example per wire shape of the API contract in [../CLAUDE.md](../CLAUDE.md).
Format: a `### <key>` heading followed by one `json` code block. Keys are `<shape>_<variant>` in snake_case.
Both test suites extract these blocks and round-trip each one through their real types (see [README.md](README.md)).
Edited only from the root window via `/contract-change`.

## Status & pairing

### status_response
```json
{
  "agent_version": "0.1.0",
  "apple_music_running": true
}
```

### pair_request
```json
{
  "code": "123456",
  "device_name": "Pixel 8"
}
```

### pair_response
```json
{
  "token": "RkFLRS1UT0tFTi1mb3ItdGVzdHMtb25seS0wMDAwMDA"
}
```

## Volume

### volume_response
```json
{
  "level": 42,
  "muted": false
}
```

### volume_request_level
```json
{
  "level": 30
}
```

### volume_request_muted
```json
{
  "muted": true
}
```

### volume_request_both
```json
{
  "level": 55,
  "muted": false
}
```

## Player

### player_state_playing
```json
{
  "status": "playing",
  "title": "Example Song",
  "artist": "Example Artist",
  "album": "Example Album",
  "duration_ms": 215000,
  "position_ms": 42000,
  "artwork_rev": 7,
  "playlist_name": null
}
```

### player_state_paused_in_playlist
```json
{
  "status": "paused",
  "title": "Another Song",
  "artist": "Another Artist",
  "album": "Another Album",
  "duration_ms": 187000,
  "position_ms": 0,
  "artwork_rev": 8,
  "playlist_name": "Road Trip"
}
```

### seek_request
```json
{
  "position_ms": 42000
}
```

## Playlists & search

### playlists_response
```json
[
  {
    "id": "pl-1",
    "name": "Favourites"
  },
  {
    "id": "pl-2",
    "name": "Road Trip"
  }
]
```

### play_playlist_request
```json
{
  "id": "pl-1"
}
```

### search_response
```json
[
  {
    "id": "1000000002",
    "title": "Example Song",
    "artist": "Example Artist",
    "album": "Example Album",
    "artwork_url": "https://is1-ssl.mzstatic.com/image/thumb/Example/100x100bb.jpg",
    "url": "https://music.apple.com/us/album/example-album/1000000001?i=1000000002"
  }
]
```

### play_track_request
```json
{
  "url": "https://music.apple.com/us/album/example-album/1000000001?i=1000000002"
}
```

## Errors

### error_unauthorized
```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Missing or invalid token"
  }
}
```

### error_bad_pairing_code
```json
{
  "error": {
    "code": "BAD_PAIRING_CODE",
    "message": "Pairing code is wrong or expired"
  }
}
```

### error_apple_music_not_running
```json
{
  "error": {
    "code": "APPLE_MUSIC_NOT_RUNNING",
    "message": "Apple Music is not running"
  }
}
```

### error_not_supported
```json
{
  "error": {
    "code": "NOT_SUPPORTED",
    "message": "Playlists are not available on this machine"
  }
}
```

### error_internal
```json
{
  "error": {
    "code": "INTERNAL",
    "message": "Unexpected error"
  }
}
```

### error_bad_request
```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "Request body is not valid JSON"
  }
}
```

### error_not_found
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "No such route"
  }
}
```

### error_method_not_allowed
```json
{
  "error": {
    "code": "METHOD_NOT_ALLOWED",
    "message": "Method not allowed"
  }
}
```

### error_no_audio_session
```json
{
  "error": {
    "code": "NO_AUDIO_SESSION",
    "message": "Apple Music has no audio session yet"
  }
}
```

## WebSocket messages

### ws_state
```json
{
  "type": "state",
  "data": {
    "status": "playing",
    "title": "Example Song",
    "artist": "Example Artist",
    "album": "Example Album",
    "duration_ms": 215000,
    "position_ms": 42000,
    "artwork_rev": 7,
    "playlist_name": null
  }
}
```

### ws_volume
```json
{
  "type": "volume",
  "data": {
    "level": 42,
    "muted": false
  }
}
```

### ws_apple_music
```json
{
  "type": "apple_music",
  "data": {
    "running": false
  }
}
```

### ws_ping
```json
{
  "type": "ping"
}
```

### ws_pong
```json
{
  "type": "pong"
}
```
