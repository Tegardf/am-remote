import 'dart:typed_data';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/player_state.dart';
import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

const _statusBody = '{"agent_version":"0.1.0","apple_music_running":true}';

DioApiClient _client(
  FakeHttpAdapter adapter, {
  String? token = fakeToken,
  UnauthorizedCallback? onUnauthorized,
}) {
  final dio = Dio()..httpClientAdapter = adapter;
  return DioApiClient(
    agent: testAgent,
    token: token,
    onUnauthorized: onUnauthorized,
    dio: dio,
  );
}

String _errorBody(String code) => '{"error":{"code":"$code","message":"msg"}}';

void main() {
  group('playlists and search', () {
    const playlistsJson =
        '[{"id":"pl-1","name":"Favourites"},{"id":"pl-2","name":"Road Trip"}]';
    const searchJson =
        '[{"id":"1000000002","title":"Example Song","artist":"Example Artist",'
        '"album":"Example Album",'
        '"artwork_url":"https://is1-ssl.mzstatic.com/image/thumb/Example/100x100bb.jpg",'
        '"url":"https://music.apple.com/us/album/example-album/1000000001?i=1000000002"}]';

    test('getPlaylists GETs /playlists and decodes the list', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(playlistsJson, 200),
      );
      final playlists = await _client(adapter).getPlaylists();

      expect(playlists.map((p) => (p.id, p.name)), [
        ('pl-1', 'Favourites'),
        ('pl-2', 'Road Trip'),
      ]);
      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('GET', '/api/v1/playlists'));
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });

    test(
      'calls that drive Apple Music wait longer than ordinary ones',
      () async {
        final calls = <(String, Future<Object?> Function(DioApiClient))>[
          ('getPlaylists', (c) => c.getPlaylists()),
          ('playPlaylist', (c) => c.playPlaylist('pl-1')),
          ('playTrack', (c) => c.playTrack('https://music.apple.com/x?i=1')),
        ];
        for (final (name, call) in calls) {
          final adapter = FakeHttpAdapter(
            (_) => FakeHttpAdapter.json(playlistsJson, 200),
          );
          await call(_client(adapter));
          expect(
            adapter.requests.single.receiveTimeout,
            uiOperationTimeout,
            reason: name,
          );
        }
        // The agent's worst case is about 41 s and it asks for at least 45 s.
        expect(
          uiOperationTimeout,
          greaterThanOrEqualTo(const Duration(seconds: 45)),
        );

        // Everything else keeps the short default.
        final adapter = FakeHttpAdapter(
          (_) => FakeHttpAdapter.json(
            '{"agent_version":"0.1.0","apple_music_running":true}',
            200,
          ),
        );
        await _client(adapter).getStatus();
        expect(
          adapter.requests.single.receiveTimeout,
          const Duration(seconds: 10),
        );
      },
    );

    test(
      'the error codes added in C-001 and C-003 are recognised, not INTERNAL',
      () async {
        for (final (code, status) in [
          ('BAD_REQUEST', 400),
          ('NOT_FOUND', 404),
          ('METHOD_NOT_ALLOWED', 405),
          ('NO_AUDIO_SESSION', 409),
        ]) {
          final adapter = FakeHttpAdapter(
            (_) => FakeHttpAdapter.json(_errorBody(code), status),
          );
          await expectLater(
            _client(adapter).playPlaylist('pl-1'),
            throwsA(
              isA<ApiException>()
                  .having((e) => e.code, 'code', code)
                  .having((e) => e.status, 'status', status),
            ),
            reason: code,
          );
        }
      },
    );

    test('getPlaylists answers NOT_SUPPORTED for 501', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(_errorBody('NOT_SUPPORTED'), 501),
      );
      await expectLater(
        _client(adapter).getPlaylists(),
        throwsA(
          isA<ApiException>()
              .having((e) => e.code, 'code', 'NOT_SUPPORTED')
              .having((e) => e.status, 'status', 501),
        ),
      );
    });

    test('an object where a list is expected is a format error', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json('{"not":"a list"}', 200),
      );
      await expectLater(
        _client(adapter).getPlaylists(),
        throwsA(isA<FormatException>()),
      );
    });

    test('a 401 on any of the four calls goes back to pairing', () async {
      final calls = <(String, Future<Object?> Function(DioApiClient))>[
        ('getPlaylists', (c) => c.getPlaylists()),
        ('playPlaylist', (c) => c.playPlaylist('pl-1')),
        ('search', (c) => c.search('ab')),
        ('playTrack', (c) => c.playTrack('https://music.apple.com/x')),
      ];
      for (final (name, call) in calls) {
        var unauthorized = 0;
        final adapter = FakeHttpAdapter(
          (_) => FakeHttpAdapter.json(_errorBody('UNAUTHORIZED'), 401),
        );
        final client = _client(adapter, onUnauthorized: () => unauthorized++);

        await expectLater(
          call(client),
          throwsA(
            isA<ApiException>().having((e) => e.code, 'code', 'UNAUTHORIZED'),
          ),
          reason: name,
        );
        expect(unauthorized, 1, reason: name);
      }
    });

    test('playPlaylist POSTs the id', () async {
      final adapter = FakeHttpAdapter((_) => ResponseBody.fromString('', 204));
      await _client(adapter).playPlaylist('pl-2');

      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('POST', '/api/v1/play/playlist'));
      expect(sent.data, {'id': 'pl-2'});
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });

    test(
      'search GETs /search with q and limit and decodes the results',
      () async {
        final adapter = FakeHttpAdapter(
          (_) => FakeHttpAdapter.json(searchJson, 200),
        );
        final results = await _client(adapter).search('blue monday');

        expect(results.single.title, 'Example Song');
        expect(results.single.artworkUrl, startsWith('https://'));
        expect(results.single.url, startsWith('https://music.apple.com/'));
        final sent = adapter.requests.single;
        expect((sent.method, sent.uri.path), ('GET', '/api/v1/search'));
        expect(sent.uri.queryParameters, {'q': 'blue monday', 'limit': '25'});
        expect(sent.headers['Authorization'], 'Bearer $fakeToken');
      },
    );

    test('search sends the limit it is given, and escapes the query', () async {
      final adapter = FakeHttpAdapter((_) => FakeHttpAdapter.json('[]', 200));
      final results = await _client(adapter).search('a&b=c', limit: 5);

      expect(results, isEmpty);
      final sent = adapter.requests.single;
      expect(sent.uri.queryParameters, {'q': 'a&b=c', 'limit': '5'});
    });

    test('playTrack POSTs the url', () async {
      final adapter = FakeHttpAdapter((_) => ResponseBody.fromString('', 204));
      const url = 'https://music.apple.com/us/album/x/1?i=2';
      await _client(adapter).playTrack(url);

      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('POST', '/api/v1/play/track'));
      expect(sent.data, {'url': url});
    });
  });

  group('player state, artwork and seek', () {
    const stateJson =
        '{"status":"playing","title":"Example Song","artist":"Example Artist",'
        '"album":"Example Album","duration_ms":215000,"position_ms":42000,'
        '"artwork_rev":7,"playlist_name":null}';

    test('getPlayerState GETs /player/state and decodes it', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(stateJson, 200),
      );
      final state = await _client(adapter).getPlayerState();

      expect(state.status, PlaybackStatus.playing);
      expect(
        (state.title, state.durationMs, state.artworkRev),
        ('Example Song', 215000, 7),
      );
      expect(state.playlistName, isNull);
      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('GET', '/api/v1/player/state'));
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });

    test('getArtwork GETs the JPEG bytes with the token', () async {
      final jpeg = Uint8List.fromList([0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]);
      final adapter = FakeHttpAdapter(
        (_) => ResponseBody.fromBytes(
          jpeg,
          200,
          headers: {
            Headers.contentTypeHeader: ['image/jpeg'],
          },
        ),
      );
      final bytes = await _client(adapter).getArtwork();

      expect(bytes, jpeg);
      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('GET', '/api/v1/player/artwork'));
      expect(sent.responseType, ResponseType.bytes);
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });

    test('getArtwork answers null for 404 (no artwork)', () async {
      final adapter = FakeHttpAdapter((_) => ResponseBody.fromString('', 404));
      expect(await _client(adapter).getArtwork(), isNull);
    });

    test('getArtwork decodes a JSON error from a binary answer', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(_errorBody('APPLE_MUSIC_NOT_RUNNING'), 409),
      );
      await expectLater(
        _client(adapter).getArtwork(),
        throwsA(
          isA<ApiException>()
              .having((e) => e.code, 'code', 'APPLE_MUSIC_NOT_RUNNING')
              .having((e) => e.status, 'status', 409),
        ),
      );
    });

    test(
      'a 401 on the artwork goes back to pairing like any other call',
      () async {
        var unauthorized = 0;
        final adapter = FakeHttpAdapter(
          (_) => FakeHttpAdapter.json(_errorBody('UNAUTHORIZED'), 401),
        );
        final client = _client(adapter, onUnauthorized: () => unauthorized++);

        await expectLater(
          client.getArtwork(),
          throwsA(
            isA<ApiException>().having((e) => e.code, 'code', 'UNAUTHORIZED'),
          ),
        );
        expect(unauthorized, 1);
      },
    );

    test('seek POSTs the position in ms', () async {
      final adapter = FakeHttpAdapter((_) => ResponseBody.fromString('', 204));
      await _client(adapter).seek(90000);

      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('POST', '/api/v1/player/seek'));
      expect(sent.data, {'position_ms': 90000});
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });
  });

  group('player and volume endpoints', () {
    test('each transport command POSTs to its path with the token', () async {
      final calls = <(String, Future<void> Function(DioApiClient))>[
        ('play', (c) => c.play()),
        ('pause', (c) => c.pause()),
        ('toggle', (c) => c.toggle()),
        ('next', (c) => c.next()),
        ('previous', (c) => c.previous()),
      ];
      for (final (name, call) in calls) {
        final adapter = FakeHttpAdapter(
          (_) => ResponseBody.fromString('', 204),
        );
        await call(_client(adapter));

        final sent = adapter.requests.single;
        expect(sent.method, 'POST', reason: name);
        expect(sent.uri.path, '/api/v1/player/$name');
        expect(sent.headers['Authorization'], 'Bearer $fakeToken');
        expect(sent.data, isNull, reason: 'no body for $name');
      }
    });

    test('getVolume GETs /volume and decodes level and muted', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json('{"level":42,"muted":true}', 200),
      );
      final volume = await _client(adapter).getVolume();

      expect((volume.level, volume.muted), (42, true));
      final sent = adapter.requests.single;
      expect((sent.method, sent.uri.path), ('GET', '/api/v1/volume'));
      expect(sent.headers['Authorization'], 'Bearer $fakeToken');
    });

    test('setVolume sends only the fields that were given', () async {
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json('{"level":30,"muted":false}', 200),
      );
      final client = _client(adapter);

      final volume = await client.setVolume(level: 30);
      await client.setVolume(muted: true);
      await client.setVolume(level: 55, muted: false);

      expect((volume.level, volume.muted), (30, false));
      expect(adapter.requests.map((r) => r.data), [
        {'level': 30},
        {'muted': true},
        {'level': 55, 'muted': false},
      ]);
      for (final sent in adapter.requests) {
        expect((sent.method, sent.uri.path), ('POST', '/api/v1/volume'));
        expect(sent.headers['Authorization'], 'Bearer $fakeToken');
      }
    });

    test('409 becomes APPLE_MUSIC_NOT_RUNNING and 501 NOT_SUPPORTED', () async {
      for (final (status, code) in [
        (409, 'APPLE_MUSIC_NOT_RUNNING'),
        (501, 'NOT_SUPPORTED'),
      ]) {
        final adapter = FakeHttpAdapter(
          (_) => FakeHttpAdapter.json(_errorBody(code), status),
        );
        await expectLater(
          _client(adapter).toggle(),
          throwsA(
            isA<ApiException>()
                .having((e) => e.code, 'code', code)
                .having((e) => e.status, 'status', status),
          ),
        );
      }
    });
  });

  test('uses the v1 base URL and a bearer token on /status', () async {
    final adapter = FakeHttpAdapter(
      (_) => FakeHttpAdapter.json(_statusBody, 200),
    );
    final status = await _client(adapter).getStatus();

    expect(status.agentVersion, '0.1.0');
    expect(status.appleMusicRunning, isTrue);
    final sent = adapter.requests.single;
    expect(sent.uri.toString(), 'http://192.168.1.20:8765/api/v1/status');
    expect(sent.headers['Authorization'], 'Bearer $fakeToken');
  });

  test('pair sends the body without an Authorization header', () async {
    final adapter = FakeHttpAdapter(
      (_) => FakeHttpAdapter.json('{"token":"$fakeToken"}', 200),
    );
    final response = await _client(adapter)
        .pair(code: '123456', deviceName: 'Pixel 8');

    expect(response.token, fakeToken);
    final sent = adapter.requests.single;
    expect(sent.method, 'POST');
    expect(sent.uri.path, '/api/v1/pair');
    expect(sent.data, {'code': '123456', 'device_name': 'Pixel 8'});
    expect(sent.headers.containsKey('Authorization'), isFalse);
  });

  test(
    'a 401 body becomes ApiException(UNAUTHORIZED) and fires the callback',
    () async {
      var unauthorized = 0;
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(_errorBody('UNAUTHORIZED'), 401),
      );
      final client = _client(adapter, onUnauthorized: () => unauthorized++);

      await expectLater(
        client.getStatus(),
        throwsA(
          isA<ApiException>()
              .having((e) => e.code, 'code', 'UNAUTHORIZED')
              .having((e) => e.status, 'status', 401),
        ),
      );
      expect(unauthorized, 1);
    },
  );

  test(
    'BAD_PAIRING_CODE on /pair does not fire the unauthorized callback',
    () async {
      var unauthorized = 0;
      final adapter = FakeHttpAdapter(
        (_) => FakeHttpAdapter.json(_errorBody('BAD_PAIRING_CODE'), 403),
      );
      final client = _client(
        adapter,
        token: null,
        onUnauthorized: () => unauthorized++,
      );

      await expectLater(
        client.pair(code: '000000', deviceName: 'Pixel 8'),
        throwsA(
          isA<ApiException>().having((e) => e.code, 'code', 'BAD_PAIRING_CODE'),
        ),
      );
      expect(unauthorized, 0);
    },
  );

  test('an unknown error code is treated as INTERNAL', () async {
    final adapter = FakeHttpAdapter(
      (_) => FakeHttpAdapter.json(_errorBody('SOMETHING_NEW'), 400),
    );
    await expectLater(
      _client(adapter).getStatus(),
      throwsA(
        isA<ApiException>()
            .having((e) => e.code, 'code', 'INTERNAL')
            .having((e) => e.status, 'status', 400),
      ),
    );
  });

  test('a non-JSON error body is treated as INTERNAL', () async {
    final adapter = FakeHttpAdapter(
      (_) => ResponseBody.fromString('Bad Request', 400),
    );
    await expectLater(
      _client(adapter).getStatus(),
      throwsA(isA<ApiException>().having((e) => e.code, 'code', 'INTERNAL')),
    );
  });

  test('204 with no body is OK and is not decoded', () async {
    final adapter = FakeHttpAdapter((_) => ResponseBody.fromString('', 204));
    final result = await _client(adapter)
        .request<Object?>('POST', '/player/play');
    expect(result, isNull);
  });

  test('no reply at all becomes AgentUnreachableException', () async {
    final adapter = FakeHttpAdapter(
      (options) => throw DioException.connectionError(
        requestOptions: options,
        reason: 'refused',
      ),
    );
    await expectLater(
      _client(adapter).getStatus(),
      throwsA(isA<AgentUnreachableException>()),
    );
  });
}
