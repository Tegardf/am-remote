import 'dart:typed_data';

import 'package:am_remote/core/thumbnail_loader.dart';
import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

final _jpeg = Uint8List.fromList([0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]);

ResponseBody _ok([Uint8List? bytes]) =>
    ResponseBody.fromBytes(bytes ?? _jpeg, 200);

ResponseBody _redirect(String location, [int status = 302]) =>
    ResponseBody.fromString(
      '',
      status,
      headers: {
        'location': [location],
      },
    );

HttpsThumbnailLoader _loader(
  FakeHttpAdapter adapter, {
  int maxBytes = 2 * 1024 * 1024,
  int cacheSize = 80,
}) => HttpsThumbnailLoader(
  dio: Dio()..httpClientAdapter = adapter,
  maxBytes: maxBytes,
  cacheSize: cacheSize,
);

void main() {
  test('loads an https picture', () async {
    final adapter = FakeHttpAdapter((_) => _ok());
    final bytes = await _loader(adapter)
        .load('https://is1-ssl.mzstatic.com/a.jpg');

    expect(bytes, _jpeg);
    final sent = adapter.requests.single;
    expect(sent.uri.toString(), 'https://is1-ssl.mzstatic.com/a.jpg');
    expect(
      sent.followRedirects,
      isFalse,
      reason: 'redirects are followed by hand',
    );
  });

  test(
    'sends no Authorization header or other secrets to the internet',
    () async {
      final adapter = FakeHttpAdapter((_) => _ok());
      await _loader(adapter).load('https://example.com/a.jpg');

      expect(
        adapter.requests.single.headers.containsKey('Authorization'),
        isFalse,
      );
    },
  );

  test(
    'never makes a request for a URL that is not https with a host',
    () async {
      final adapter = FakeHttpAdapter((_) => _ok());
      final loader = _loader(adapter);

      for (final url in [
        '',
        'http://example.com/a.jpg',
        'HTTP://example.com/a.jpg',
        'ftp://example.com/a.jpg',
        'file:///sdcard/a.jpg',
        'javascript:alert(1)',
        'https:',
        'https:foo',
        'https:///a.jpg',
        '//example.com/a.jpg',
        'example.com/a.jpg',
        'not a url',
      ]) {
        expect(await loader.load(url), isNull, reason: url);
      }
      expect(adapter.requests, isEmpty);
    },
  );

  test('accepts the scheme in any letter case', () async {
    final adapter = FakeHttpAdapter((_) => _ok());
    expect(await _loader(adapter).load('HTTPS://example.com/a.jpg'), _jpeg);
  });

  test('follows a redirect to another https address', () async {
    final adapter = FakeHttpAdapter(
      (o) => o.uri.path == '/a.jpg'
          ? _redirect('https://cdn.example.com/b.jpg')
          : _ok(),
    );
    final bytes = await _loader(adapter).load('https://example.com/a.jpg');

    expect(bytes, _jpeg);
    expect(adapter.requests.map((r) => r.uri.toString()), [
      'https://example.com/a.jpg',
      'https://cdn.example.com/b.jpg',
    ]);
  });

  test('follows a relative redirect on the same host', () async {
    final adapter = FakeHttpAdapter(
      (o) => o.uri.path == '/a.jpg' ? _redirect('/moved/b.jpg', 301) : _ok(),
    );
    final bytes = await _loader(adapter).load('https://example.com/a.jpg');

    expect(bytes, _jpeg);
    expect(
      adapter.requests.last.uri.toString(),
      'https://example.com/moved/b.jpg',
    );
  });

  test(
    'refuses a redirect from https down to http, without requesting it',
    () async {
      final adapter = FakeHttpAdapter(
        (_) => _redirect('http://evil.example.com/a.jpg'),
      );
      final bytes = await _loader(adapter).load('https://example.com/a.jpg');

      expect(bytes, isNull);
      expect(
        adapter.requests,
        hasLength(1),
        reason: 'the http:// hop is never made',
      );
    },
  );

  test(
    'refuses a redirect to another odd scheme, and one without a Location',
    () async {
      for (final location in [
        'ftp://example.com/a.jpg',
        'file:///a.jpg',
        'https:',
      ]) {
        final adapter = FakeHttpAdapter((_) => _redirect(location));
        expect(
          await _loader(adapter).load('https://example.com/a.jpg'),
          isNull,
          reason: location,
        );
        expect(adapter.requests, hasLength(1));
      }
      final noLocation = FakeHttpAdapter(
        (_) => ResponseBody.fromString('', 302),
      );
      expect(
        await _loader(noLocation).load('https://example.com/a.jpg'),
        isNull,
      );
    },
  );

  test('gives up after too many redirects', () async {
    final adapter = FakeHttpAdapter(
      (o) => _redirect('https://example.com/loop${o.uri.path}'),
    );
    final bytes = await _loader(adapter).load('https://example.com/a.jpg');

    expect(bytes, isNull);
    // The first request plus 3 redirects.
    expect(adapter.requests, hasLength(4));
  });

  test(
    'a missing picture, an error, or an empty or huge answer gives null',
    () async {
      for (final response in [
        () => ResponseBody.fromString('', 404),
        () => ResponseBody.fromString('', 500),
        () => ResponseBody.fromBytes(Uint8List(0), 200),
        () => ResponseBody.fromBytes(Uint8List(5000), 200),
      ]) {
        final adapter = FakeHttpAdapter((_) => response());
        expect(
          await _loader(
            adapter,
            maxBytes: 1000,
          ).load('https://example.com/a.jpg'),
          isNull,
        );
      }
    },
  );

  test('a connection error gives null instead of throwing', () async {
    final adapter = FakeHttpAdapter(
      (o) => throw DioException.connectionError(
        requestOptions: o,
        reason: 'offline',
      ),
    );
    expect(await _loader(adapter).load('https://example.com/a.jpg'), isNull);
  });

  test(
    'a picture is only requested once, even asked for at the same time',
    () async {
      final adapter = FakeHttpAdapter((_) => _ok());
      final loader = _loader(adapter);
      const url = 'https://example.com/a.jpg';

      final results = await Future.wait([loader.load(url), loader.load(url)]);
      await loader.load(url);

      expect(results, [_jpeg, _jpeg]);
      expect(adapter.requests, hasLength(1));
    },
  );

  test('a failed picture is asked for again next time', () async {
    var fail = true;
    final adapter = FakeHttpAdapter(
      (_) => fail ? ResponseBody.fromString('', 500) : _ok(),
    );
    final loader = _loader(adapter);

    expect(await loader.load('https://example.com/a.jpg'), isNull);
    fail = false;
    expect(await loader.load('https://example.com/a.jpg'), _jpeg);
  });

  test('the cache forgets the least recently used picture', () async {
    final adapter = FakeHttpAdapter((_) => _ok());
    final loader = _loader(adapter, cacheSize: 2);

    await loader.load('https://example.com/1.jpg');
    await loader.load('https://example.com/2.jpg');
    await loader.load('https://example.com/1.jpg'); // 1 is fresh again
    await loader.load('https://example.com/3.jpg'); // pushes 2 out
    expect(adapter.requests, hasLength(3));

    await loader.load('https://example.com/1.jpg'); // still cached
    expect(adapter.requests, hasLength(3));
    await loader.load('https://example.com/2.jpg'); // fetched again
    expect(adapter.requests, hasLength(4));
  });
}
