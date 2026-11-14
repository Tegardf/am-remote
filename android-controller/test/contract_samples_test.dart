import 'dart:convert';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/agent_status.dart';
import 'package:am_remote/core/models/api_error_body.dart';
import 'package:am_remote/core/models/pair_request.dart';
import 'package:am_remote/core/models/pair_response.dart';
import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/core/models/playlist.dart';
import 'package:am_remote/core/models/search_result.dart';
import 'package:am_remote/core/models/ws_message.dart';
import 'package:am_remote/core/models/volume.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

/// Every sample this side models must survive fromJson → toJson unchanged.
/// When you add a model, add its sample keys here.
void main() {
  final samples = loadSamples();

  /// Sample keys a test below covers. The last test fails when `samples.md`
  /// has a key that is in neither this set nor [notModelled], so a sample
  /// added to the contract can't be forgotten.
  final covered = <String>{};

  /// Samples this app deliberately has no type for, each with the reason.
  /// (None today: the app models all 28.)
  const notModelled = <String, String>{};

  void roundTrip(
    String key,
    Object Function(Map<String, Object?>) fromJson,
    Object? Function(Object) toJson,
  ) {
    covered.add(key);
    test('sample $key round-trips', () {
      final raw = samples[key];
      expect(raw, isNotNull, reason: 'missing sample $key in samples.md');
      final json = jsonDecode(raw!) as Map<String, Object?>;
      expect(toJson(fromJson(json)), equals(json));
    });
  }

  roundTrip(
    'status_response',
    AgentStatus.fromJson,
    (m) => (m as AgentStatus).toJson(),
  );
  roundTrip(
    'pair_request',
    PairRequest.fromJson,
    (m) => (m as PairRequest).toJson(),
  );
  roundTrip(
    'pair_response',
    PairResponse.fromJson,
    (m) => (m as PairResponse).toJson(),
  );

  roundTrip('volume_response', Volume.fromJson, (m) => (m as Volume).toJson());
  for (final key in [
    'volume_request_level',
    'volume_request_muted',
    'volume_request_both',
  ]) {
    roundTrip(
      key,
      VolumeRequest.fromJson,
      (m) => (m as VolumeRequest).toJson(),
    );
  }

  for (final key in [
    'player_state_playing',
    'player_state_paused_in_playlist',
  ]) {
    roundTrip(key, PlayerState.fromJson, (m) => (m as PlayerState).toJson());
  }
  roundTrip(
    'seek_request',
    SeekRequest.fromJson,
    (m) => (m as SeekRequest).toJson(),
  );
  final wsKeys = samples.keys.where((k) => k.startsWith('ws_')).toList();
  test('samples.md has the five ws_* samples', () {
    expect(
      wsKeys,
      containsAll([
        'ws_state',
        'ws_volume',
        'ws_apple_music',
        'ws_ping',
        'ws_pong',
      ]),
    );
  });
  for (final key in wsKeys) {
    roundTrip(key, WsMessage.fromJson, (m) => (m as WsMessage).toJson());
  }

  // `playlists_response` and `search_response` are JSON arrays.
  void roundTripList(
    String key,
    Object Function(Map<String, Object?>) fromJson,
    Object? Function(Object) toJson,
  ) {
    covered.add(key);
    test('sample $key round-trips', () {
      final raw = samples[key];
      expect(raw, isNotNull, reason: 'missing sample $key in samples.md');
      final list = (jsonDecode(raw!) as List).cast<Map<String, Object?>>();
      expect(list, isNotEmpty);
      expect([for (final item in list) toJson(fromJson(item))], equals(list));
    });
  }

  roundTripList(
    'playlists_response',
    Playlist.fromJson,
    (m) => (m as Playlist).toJson(),
  );
  roundTrip(
    'play_playlist_request',
    PlayPlaylistRequest.fromJson,
    (m) => (m as PlayPlaylistRequest).toJson(),
  );
  roundTripList(
    'search_response',
    SearchResult.fromJson,
    (m) => (m as SearchResult).toJson(),
  );
  roundTrip(
    'play_track_request',
    PlayTrackRequest.fromJson,
    (m) => (m as PlayTrackRequest).toJson(),
  );

  test('a sparse search result (no artist, album or artwork) still parses', () {
    final result = SearchResult.fromJson({
      'id': '1',
      'title': 'Only a title',
      'artist': null,
      'url': 'https://music.apple.com/x',
    });
    expect((result.artist, result.album, result.artworkUrl), ('', '', ''));
    expect(result.url, 'https://music.apple.com/x');
  });

  test('an unknown status value maps to "unknown"', () {
    final state = PlayerState.fromJson({
      'status': 'buffering-on-mars',
      'title': 'T',
    });
    expect(state.status, PlaybackStatus.unknown);
    expect(state.title, 'T');
  });

  test('every known status parses', () {
    for (final status in PlaybackStatus.values) {
      expect(PlayerState.fromJson({'status': status.name}).status, status);
    }
  });

  test('an idle player (null or missing fields) still parses', () {
    final state = PlayerState.fromJson({
      'status': 'stopped',
      'title': null,
      'artist': null,
      'duration_ms': null,
    });
    expect(state.title, '');
    expect(state.artist, '');
    expect((state.durationMs, state.positionMs, state.artworkRev), (0, 0, 0));
    expect(state.playlistName, isNull);
  });

  test('parseWsEvent understands the contract samples', () {
    WsEvent? parse(String key) => parseWsEvent(
      WsMessage.fromJson(jsonDecode(samples[key]!) as Map<String, Object?>),
    );
    expect(parse('ws_state'), isA<StateEvent>());
    expect(parse('ws_volume'), isA<VolumeEvent>());
    expect((parse('ws_apple_music') as AppleMusicEvent).running, isFalse);
    expect(parse('ws_pong'), isA<PongEvent>());
    // ping is client → server only; the app has no event for it.
    expect(parse('ws_ping'), isNull);
    expect(parseWsEvent(const WsMessage(type: 'something_new')), isNull);
  });

  test('unknown JSON fields are ignored (forward compatible)', () {
    final status = AgentStatus.fromJson({
      'agent_version': '9.9.9',
      'apple_music_running': false,
      'some_future_field': 1,
    });
    expect(status.agentVersion, '9.9.9');

    final pair = PairResponse.fromJson({'token': 'x', 'expires_in': 5});
    expect(pair.token, 'x');

    final error = ApiErrorBody.fromJson({
      'error': {'code': 'INTERNAL', 'message': 'm', 'hint': 'future'},
      'request_id': 'abc',
    });
    expect(error.error.code, 'INTERNAL');
  });

  final errorKeys = samples.keys.where((k) => k.startsWith('error_')).toList();
  test('samples.md has error samples', () => expect(errorKeys, isNotEmpty));
  for (final key in errorKeys) {
    roundTrip(key, ApiErrorBody.fromJson, (m) => (m as ApiErrorBody).toJson());

    // A code the app doesn't know would silently become INTERNAL.
    test('sample $key has a code the app knows', () {
      final body = ApiErrorBody.fromJson(
        jsonDecode(samples[key]!) as Map<String, Object?>,
      );
      expect(ApiErrorCode.known, contains(body.error.code));
    });
  }

  test('every sample in samples.md is covered by a test above', () {
    final forgotten = samples.keys.toSet()
      ..removeAll(covered)
      ..removeAll(notModelled.keys);
    expect(
      forgotten,
      isEmpty,
      reason:
          'samples.md has samples no test round-trips: add them to '
          'test/contract_samples_test.dart (or to notModelled, with a reason)',
    );
    // A stale entry would hide a sample that was removed or renamed.
    expect(
      covered
          .followedBy(notModelled.keys)
          .toSet()
          .difference(samples.keys.toSet()),
      isEmpty,
      reason: 'the test covers samples that are not in samples.md',
    );
  });

  test('the contract has the codes added in C-001 and C-003', () {
    for (final key in [
      'error_bad_request',
      'error_not_found',
      'error_method_not_allowed',
      'error_no_audio_session',
    ]) {
      expect(errorKeys, contains(key));
    }
  });
}
