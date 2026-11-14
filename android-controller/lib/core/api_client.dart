import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';

import 'agent_address.dart';
import 'models/agent_status.dart';
import 'models/api_error_body.dart';
import 'models/pair_request.dart';
import 'models/pair_response.dart';
import 'models/player_state.dart';
import 'models/playlist.dart';
import 'models/search_result.dart';
import 'models/volume.dart';

/// Error codes of the contract. UI logic switches on these, never on messages.
abstract final class ApiErrorCode {
  static const unauthorized = 'UNAUTHORIZED';
  static const badPairingCode = 'BAD_PAIRING_CODE';
  static const badRequest = 'BAD_REQUEST';
  static const notFound = 'NOT_FOUND';
  static const methodNotAllowed = 'METHOD_NOT_ALLOWED';
  static const appleMusicNotRunning = 'APPLE_MUSIC_NOT_RUNNING';

  /// Apple Music has a media session, but Windows has no audio session for it
  /// yet. Only `GET` and `POST /volume` answer it; transport commands work.
  static const noAudioSession = 'NO_AUDIO_SESSION';
  static const notSupported = 'NOT_SUPPORTED';
  static const internal = 'INTERNAL';

  static const known = {
    unauthorized,
    badPairingCode,
    badRequest,
    notFound,
    methodNotAllowed,
    appleMusicNotRunning,
    noAudioSession,
    notSupported,
    internal,
  };
}

/// A non-2xx answer from the agent, turned into a typed error.
/// Unknown codes (and bodies that aren't the error JSON) become INTERNAL.
class ApiException implements Exception {
  const ApiException(this.code, this.message, [this.status]);

  final String code;
  final String message;
  final int? status;

  bool get isUnauthorized => code == ApiErrorCode.unauthorized;

  @override
  String toString() => 'ApiException($code, status: $status): $message';
}

/// The agent could not be reached at all (refused, timeout, Wi-Fi off).
class AgentUnreachableException implements Exception {
  const AgentUnreachableException([this.message = 'Agent unreachable']);

  final String message;

  @override
  String toString() => 'AgentUnreachableException: $message';
}

/// What the app can ask the agent. Screens and controllers depend on this
/// interface, so tests use a fake instead of the network.
/// Failures surface as [ApiException] or [AgentUnreachableException].
abstract interface class ApiClient {
  Future<AgentStatus> getStatus();

  Future<PairResponse> pair({required String code, required String deviceName});

  /// `POST /player/play|pause|toggle|next|previous`: each answers 204.
  Future<void> play();

  Future<void> pause();

  Future<void> toggle();

  Future<void> next();

  Future<void> previous();

  Future<Volume> getVolume();

  /// `POST /volume`. Send [level] (0-100), [muted], or both.
  Future<Volume> setVolume({int? level, bool? muted});

  Future<PlayerState> getPlayerState();

  /// `GET /player/artwork`: the JPEG bytes, or null when the agent has none (404).
  Future<Uint8List?> getArtwork();

  /// `POST /player/seek`.
  Future<void> seek(int positionMs);

  /// `GET /playlists`. May answer `NOT_SUPPORTED` (501) when the agent
  /// can't read them.
  Future<List<Playlist>> getPlaylists();

  /// `POST /play/playlist`.
  Future<void> playPlaylist(String id);

  /// `GET /search?q=&limit=`.
  Future<List<SearchResult>> search(String query, {int limit = 25});

  /// `POST /play/track` with a result's `url`.
  Future<void> playTrack(String url);
}

/// How long to wait for calls where the agent drives Apple Music's window
/// (playlists, playing a playlist or a song). The agent's worst case is about
/// 41 s (35 s of driving the window plus 6 s waiting for playback to start),
/// and it asks clients for at least 45 s, or a slow success looks like a
/// failure.
const uiOperationTimeout = Duration(seconds: 45);

/// Called after any authenticated call is answered with UNAUTHORIZED.
typedef UnauthorizedCallback = void Function();

class DioApiClient implements ApiClient {
  DioApiClient({
    required AgentAddress agent,
    this._token,
    this.onUnauthorized,
    Dio? dio,
  }) : _dio = dio ?? Dio() {
    _dio.options
      ..baseUrl = 'http://${agent.host}:${agent.port}/api/v1'
      ..connectTimeout = const Duration(seconds: 5)
      ..receiveTimeout = const Duration(seconds: 10)
      ..responseType = ResponseType.json
      // Handle every status ourselves so error JSON can be mapped in one place.
      ..validateStatus = (_) => true;
    _dio.interceptors.add(
      InterceptorsWrapper(
        onRequest: (options, handler) {
          final token = _token;
          if (token != null && options.path != _pairPath) {
            options.headers['Authorization'] = 'Bearer $token';
          }
          handler.next(options);
        },
      ),
    );
  }

  static const _pairPath = '/pair';

  final String? _token;
  final Dio _dio;
  final UnauthorizedCallback? onUnauthorized;

  @override
  Future<AgentStatus> getStatus() async {
    final data = await request<Object?>('GET', '/status');
    return AgentStatus.fromJson(_asJsonMap(data));
  }

  @override
  Future<PairResponse> pair({
    required String code,
    required String deviceName,
  }) async {
    final body = PairRequest(code: code, deviceName: deviceName).toJson();
    final data = await request<Object?>('POST', _pairPath, body: body);
    return PairResponse.fromJson(_asJsonMap(data));
  }

  @override
  Future<void> play() => request<Object?>('POST', '/player/play');

  @override
  Future<void> pause() => request<Object?>('POST', '/player/pause');

  @override
  Future<void> toggle() => request<Object?>('POST', '/player/toggle');

  @override
  Future<void> next() => request<Object?>('POST', '/player/next');

  @override
  Future<void> previous() => request<Object?>('POST', '/player/previous');

  @override
  Future<Volume> getVolume() async {
    final data = await request<Object?>('GET', '/volume');
    return Volume.fromJson(_asJsonMap(data));
  }

  @override
  Future<Volume> setVolume({int? level, bool? muted}) async {
    final body = VolumeRequest(level: level, muted: muted).toJson();
    final data = await request<Object?>('POST', '/volume', body: body);
    return Volume.fromJson(_asJsonMap(data));
  }

  @override
  Future<PlayerState> getPlayerState() async {
    final data = await request<Object?>('GET', '/player/state');
    return PlayerState.fromJson(_asJsonMap(data));
  }

  @override
  Future<Uint8List?> getArtwork() async {
    try {
      final data = await request<List<int>>(
        'GET',
        '/player/artwork',
        responseType: ResponseType.bytes,
      );
      if (data == null || data.isEmpty) return null;
      return data is Uint8List ? data : Uint8List.fromList(data);
    } on ApiException catch (e) {
      if (e.status == 404) return null;
      rethrow;
    }
  }

  @override
  Future<void> seek(int positionMs) => request<Object?>(
    'POST',
    '/player/seek',
    body: SeekRequest(positionMs: positionMs).toJson(),
  );

  @override
  Future<List<Playlist>> getPlaylists() async {
    final data = await request<Object?>(
      'GET',
      '/playlists',
      receiveTimeout: uiOperationTimeout,
    );
    return _asJsonList(data).map(Playlist.fromJson).toList();
  }

  @override
  Future<void> playPlaylist(String id) => request<Object?>(
    'POST',
    '/play/playlist',
    body: PlayPlaylistRequest(id: id).toJson(),
    receiveTimeout: uiOperationTimeout,
  );

  @override
  Future<List<SearchResult>> search(String query, {int limit = 25}) async {
    final data = await request<Object?>(
      'GET',
      '/search',
      query: {'q': query, 'limit': limit},
    );
    return _asJsonList(data).map(SearchResult.fromJson).toList();
  }

  @override
  Future<void> playTrack(String url) => request<Object?>(
    'POST',
    '/play/track',
    body: PlayTrackRequest(url: url).toJson(),
    receiveTimeout: uiOperationTimeout,
  );

  /// The one place requests are sent and errors mapped; every endpoint method
  /// goes through it. 2xx returns the decoded body (null for 204); anything
  /// else throws [ApiException], or [AgentUnreachableException] without a reply.
  /// Use [responseType] `bytes` for binary bodies such as the artwork.
  Future<T?> request<T>(
    String method,
    String path, {
    Object? body,
    Map<String, Object?>? query,
    ResponseType? responseType,
    Duration? receiveTimeout,
  }) async {
    final Response<Object?> response;
    try {
      response = await _dio.request<Object?>(
        path,
        data: body,
        queryParameters: query,
        options: Options(
          method: method,
          responseType: responseType,
          receiveTimeout: receiveTimeout,
        ),
      );
    } on DioException catch (e) {
      throw AgentUnreachableException(e.type.name);
    }

    final status = response.statusCode ?? 0;
    if (status >= 200 && status < 300) {
      if (status == 204) return null;
      return response.data as T?;
    }

    final exception = _toApiException(status, response.data);
    if (exception.isUnauthorized && path != _pairPath) {
      onUnauthorized?.call();
    }
    throw exception;
  }

  static ApiException _toApiException(int status, Object? data) {
    try {
      // An error answer to a binary request still carries the JSON error body.
      final body = data is List<int> ? jsonDecode(utf8.decode(data)) : data;
      final detail = ApiErrorBody.fromJson(_asJsonMap(body)).error;
      final code = ApiErrorCode.known.contains(detail.code)
          ? detail.code
          : ApiErrorCode.internal;
      return ApiException(code, detail.message, status);
    } catch (_) {
      return ApiException(
        ApiErrorCode.internal,
        'Unexpected response from the laptop',
        status,
      );
    }
  }

  static Map<String, Object?> _asJsonMap(Object? data) {
    if (data is Map) return Map<String, Object?>.from(data);
    throw const FormatException('Expected a JSON object');
  }

  /// A JSON array of objects. A body of the wrong shape is a format error.
  static List<Map<String, Object?>> _asJsonList(Object? data) {
    if (data is! List) throw const FormatException('Expected a JSON array');
    return [for (final item in data) _asJsonMap(item)];
  }
}
