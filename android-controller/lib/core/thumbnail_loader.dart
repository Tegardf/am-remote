import 'dart:typed_data';

import 'package:dio/dio.dart';

/// Loads the small artwork pictures of search results. Tests use a fake.
abstract interface class ThumbnailLoader {
  /// The image bytes, or null if the picture can't be had.
  Future<Uint8List?> load(String url);
}

/// Loads pictures from the internet over HTTPS only.
///
/// `Image.network` follows redirects, including `https` -> `http`, and the app
/// allows cleartext traffic for the laptop on the LAN. So pictures are fetched
/// here instead: every hop, including each redirect, must be `https://` with a
/// host, or the picture is dropped. No token or other header is ever sent.
class HttpsThumbnailLoader implements ThumbnailLoader {
  HttpsThumbnailLoader({
    Dio? dio,
    this.maxBytes = 2 * 1024 * 1024,
    this.maxRedirects = 3,
    this.cacheSize = 80,
  }) : _dio = dio ?? Dio();

  final Dio _dio;

  /// Larger answers are dropped.
  final int maxBytes;
  final int maxRedirects;
  final int cacheSize;

  // Least recently used pictures, kept in memory so scrolling back is instant.
  // A map literal keeps insertion order, so the first key is the oldest.
  final _cache = <String, Uint8List>{};
  final _inFlight = <String, Future<Uint8List?>>{};

  @override
  Future<Uint8List?> load(String url) {
    final cached = _cache.remove(url);
    if (cached != null) {
      _cache[url] = cached;
      return Future.value(cached);
    }
    // A block body on purpose: `Map.remove` returns this very future, and a
    // `whenComplete` callback that returns a future is waited for, so an
    // arrow function here would wait for itself forever.
    return _inFlight[url] ??= _fetch(url).whenComplete(() {
      _inFlight.remove(url);
    });
  }

  Future<Uint8List?> _fetch(String url) async {
    var uri = httpsUri(url);
    if (uri == null) return null;
    try {
      for (var hop = 0; hop <= maxRedirects; hop++) {
        final response = await _dio.getUri<List<int>>(
          uri!,
          options: Options(
            responseType: ResponseType.bytes,
            followRedirects: false,
            validateStatus: (_) => true,
            receiveTimeout: const Duration(seconds: 10),
          ),
        );
        final status = response.statusCode ?? 0;
        if (status >= 300 && status < 400) {
          final location = response.headers.value('location');
          if (location == null) return null;
          // A redirect to http:// (or anything but https://) ends here.
          uri = httpsUri(uri.resolve(location).toString());
          if (uri == null) return null;
          continue;
        }
        if (status != 200) return null;
        final data = response.data;
        if (data == null || data.isEmpty || data.length > maxBytes) return null;
        final bytes = data is Uint8List ? data : Uint8List.fromList(data);
        _remember(url, bytes);
        return bytes;
      }
      return null; // too many redirects
    } catch (_) {
      return null;
    }
  }

  void _remember(String url, Uint8List bytes) {
    _cache[url] = bytes;
    while (_cache.length > cacheSize) {
      _cache.remove(_cache.keys.first);
    }
  }

  /// [url] as a [Uri] if it is a usable `https://` address with a host.
  static Uri? httpsUri(String url) {
    final uri = Uri.tryParse(url);
    if (uri == null || uri.scheme != 'https' || uri.host.isEmpty) return null;
    return uri;
  }
}
