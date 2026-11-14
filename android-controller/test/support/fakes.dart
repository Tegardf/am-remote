import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:am_remote/core/agent_address.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/discovery.dart';
import 'package:am_remote/core/models/agent_status.dart';
import 'package:am_remote/core/models/pair_response.dart';
import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/core/models/playlist.dart';
import 'package:am_remote/core/models/search_result.dart';
import 'package:am_remote/core/models/volume.dart';
import 'package:am_remote/core/providers.dart';
import 'package:am_remote/core/session_storage.dart';
import 'package:am_remote/core/thumbnail_loader.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/player/live_controller.dart';
import 'package:dio/dio.dart';
import 'package:flutter_riverpod/misc.dart' show Override;

/// Obviously fake token (same one as in ../contract/samples.md).
const fakeToken = 'RkFLRS1UT0tFTi1mb3ItdGVzdHMtb25seS0wMDAwMDA';

const testAgent = AgentAddress(host: '192.168.1.20', name: 'LAPTOP');

/// `../contract/samples.md` split into `### key` → the json block under it.
/// `flutter test` runs from the package root, so the path is relative to it.
Map<String, String> loadSamples() {
  final lines = File('../contract/samples.md').readAsLinesSync();
  final samples = <String, String>{};
  String? key;
  StringBuffer? block;
  for (final line in lines) {
    if (line.startsWith('### ')) {
      key = line.substring(4).trim();
    } else if (key != null && line.trim() == '```json') {
      block = StringBuffer();
    } else if (block != null && line.trim() == '```') {
      samples[key!] = block.toString();
      block = null;
      key = null;
    } else {
      block?.writeln(line);
    }
  }
  return samples;
}

class FakeApiClient implements ApiClient {
  Object? statusError;
  AgentStatus status = const AgentStatus(
    agentVersion: '0.1.0',
    appleMusicRunning: true,
  );
  int statusCalls = 0;

  Object? pairError;
  String pairToken = fakeToken;
  final pairCalls = <({String code, String deviceName})>[];

  /// Names of the transport commands received, in order.
  final calls = <String>[];

  /// Thrown by every transport command and by volume calls when set.
  Object? commandError;

  Volume volume = const Volume(level: 42, muted: false);
  int getVolumeCalls = 0;
  final setVolumeCalls = <({int? level, bool? muted})>[];

  @override
  Future<AgentStatus> getStatus() async {
    statusCalls++;
    final error = statusError;
    if (error != null) throw error;
    return status;
  }

  /// When set, transport commands wait for it: simulates a slow request.
  Completer<void>? gate;

  Future<void> _command(String name) async {
    calls.add(name);
    await gate?.future;
    final error = commandError;
    if (error != null) throw error;
  }

  @override
  Future<void> play() => _command('play');

  @override
  Future<void> pause() => _command('pause');

  @override
  Future<void> toggle() => _command('toggle');

  @override
  Future<void> next() => _command('next');

  @override
  Future<void> previous() => _command('previous');

  PlayerState playerState = const PlayerState();
  int getPlayerStateCalls = 0;

  /// `getArtwork` answers these bytes; null means "no artwork" (404).
  Uint8List? artwork = Uint8List.fromList(const [1, 2, 3]);
  int artworkCalls = 0;
  Completer<void>? artworkGate;

  /// Thrown by `getArtwork` when set (a failed request, not a 404).
  Object? artworkError;
  final seekCalls = <int>[];

  @override
  Future<PlayerState> getPlayerState() async {
    getPlayerStateCalls++;
    return playerState;
  }

  @override
  Future<Uint8List?> getArtwork() async {
    artworkCalls++;
    // What the laptop had when asked, even if the answer arrives late.
    final snapshot = artwork;
    await artworkGate?.future;
    final error = artworkError;
    if (error != null) throw error;
    return snapshot;
  }

  @override
  Future<void> seek(int positionMs) async {
    seekCalls.add(positionMs);
    final error = commandError;
    if (error != null) throw error;
  }

  // Playlists and search.
  List<Playlist> playlists = const [
    Playlist(id: 'pl-1', name: 'Favourites'),
    Playlist(id: 'pl-2', name: 'Road Trip'),
  ];
  int getPlaylistsCalls = 0;
  Completer<void>? playlistsGate;
  Object? playlistsError;
  final playedPlaylists = <String>[];

  List<SearchResult> searchResults = const [
    SearchResult(
      id: '1000000002',
      title: 'Example Song',
      artist: 'Example Artist',
      album: 'Example Album',
      artworkUrl:
          'https://is1-ssl.mzstatic.com/image/thumb/Example/100x100bb.jpg',
      url: 'https://music.apple.com/us/album/example-album/1000000001?i=1000000002',
    ),
  ];
  final searchCalls = <({String query, int limit})>[];
  Completer<void>? searchGate;
  Object? searchError;
  final playedTracks = <String>[];

  /// Thrown by `playPlaylist` and `playTrack` when set.
  Object? playError;

  /// When set, `playPlaylist` and `playTrack` wait for it: a slow request.
  Completer<void>? playGate;

  @override
  Future<List<Playlist>> getPlaylists() async {
    getPlaylistsCalls++;
    // What the laptop had when asked, even if the answer arrives late.
    final snapshot = playlists;
    await playlistsGate?.future;
    final error = playlistsError;
    if (error != null) throw error;
    return snapshot;
  }

  @override
  Future<void> playPlaylist(String id) async {
    playedPlaylists.add(id);
    await playGate?.future;
    final error = playError;
    if (error != null) throw error;
  }

  @override
  Future<List<SearchResult>> search(String query, {int limit = 25}) async {
    searchCalls.add((query: query, limit: limit));
    // The answer the laptop had when asked, even if it arrives late.
    final snapshot = searchResults;
    await searchGate?.future;
    final error = searchError;
    if (error != null) throw error;
    return snapshot;
  }

  @override
  Future<void> playTrack(String url) async {
    playedTracks.add(url);
    await playGate?.future;
    final error = playError;
    if (error != null) throw error;
  }

  /// Thrown by `getVolume` / `setVolume` when set (checked after the gates).
  Object? volumeError;

  /// When set, `getVolume` / `setVolume` wait for it: simulates a slow request.
  Completer<void>? getVolumeGate;
  Completer<void>? setVolumeGate;

  @override
  Future<Volume> getVolume() async {
    getVolumeCalls++;
    // What the laptop answered at the time of the request, even if the answer
    // arrives late.
    final snapshot = volume;
    await getVolumeGate?.future;
    final error = volumeError;
    if (error != null) throw error;
    return snapshot;
  }

  @override
  Future<Volume> setVolume({int? level, bool? muted}) async {
    setVolumeCalls.add((level: level, muted: muted));
    await setVolumeGate?.future;
    final error = volumeError;
    if (error != null) throw error;
    volume = Volume(level: level ?? volume.level, muted: muted ?? volume.muted);
    return volume;
  }

  @override
  Future<PairResponse> pair({
    required String code,
    required String deviceName,
  }) async {
    pairCalls.add((code: code, deviceName: deviceName));
    final error = pairError;
    if (error != null) throw error;
    return PairResponse(token: pairToken);
  }
}

class InMemorySessionStorage implements SessionStorage {
  InMemorySessionStorage({this.token, this.agent});

  String? token;
  AgentAddress? agent;

  @override
  Future<String?> readToken() async => token;

  @override
  Future<void> writeToken(String token) async => this.token = token;

  @override
  Future<void> deleteToken() async => token = null;

  @override
  Future<AgentAddress?> readAgent() async => agent;

  @override
  Future<void> writeAgent(AgentAddress agent) async => this.agent = agent;
}

class FakeDiscovery implements AgentDiscovery {
  FakeDiscovery([this.agents = const []]);

  final List<DiscoveredAgent> agents;
  int searches = 0;

  @override
  Stream<DiscoveredAgent> discover({
    Duration timeout = const Duration(seconds: 8),
  }) {
    searches++;
    return Stream.fromIterable(agents);
  }
}

/// A WebSocket connection driven by the test: it plays the agent.
class FakeWsConnection implements WsConnection {
  final _controller = StreamController<String>();

  /// Frames the app sent.
  final sent = <String>[];
  bool closed = false;

  @override
  Stream<String> get messages => _controller.stream;

  @override
  void send(String text) => sent.add(text);

  @override
  Future<void> close() async {
    closed = true;
    if (!_controller.isClosed) await _controller.close();
  }

  /// The agent sends a frame.
  void serverSend(String text) => _controller.add(text);

  /// The agent drops the connection.
  void serverDrop() => _controller.close();

  /// Types of the `ping` frames the app sent so far.
  int get pings => sent
      .where((s) => (jsonDecode(s) as Map<String, Object?>)['type'] == 'ping')
      .length;
}

/// Hands out [FakeWsConnection]s; the first [failFirst] attempts are refused.
class FakeWsConnector {
  FakeWsConnector({this.failFirst = 0});

  int failFirst;
  int attempts = 0;
  final connections = <FakeWsConnection>[];

  FakeWsConnection get last => connections.last;

  Future<WsConnection> call(Uri uri) async {
    attempts++;
    if (attempts <= failFirst) throw Exception('connection refused');
    final connection = FakeWsConnection();
    connections.add(connection);
    return connection;
  }
}

/// `{"type":"state","data":{…}}` for a test, with the contract's field names.
String wsStateFrame({
  String status = 'playing',
  String title = 'Example Song',
  String artist = 'Example Artist',
  String album = 'Example Album',
  int durationMs = 215000,
  int positionMs = 42000,
  int artworkRev = 7,
  String? playlistName,
}) => jsonEncode({
  'type': 'state',
  'data': {
    'status': status,
    'title': title,
    'artist': artist,
    'album': album,
    'duration_ms': durationMs,
    'position_ms': positionMs,
    'artwork_rev': artworkRev,
    'playlist_name': playlistName,
  },
});

String wsFrame(String type, [Map<String, Object?>? data]) =>
    jsonEncode({'type': type, 'data': ?data});

/// Stands in for the picture loader: no network, and it records what was asked.
class FakeThumbnailLoader implements ThumbnailLoader {
  FakeThumbnailLoader([this.bytes]);

  /// What every picture "loads" as; null means "can't be had".
  final Uint8List? bytes;
  final urls = <String>[];

  @override
  Future<Uint8List?> load(String url) async {
    urls.add(url);
    return bytes;
  }
}

/// A dio adapter that answers from a function, so no socket is opened.
class FakeHttpAdapter implements HttpClientAdapter {
  FakeHttpAdapter(this.handler);

  final ResponseBody Function(RequestOptions options) handler;
  final requests = <RequestOptions>[];

  static ResponseBody json(String body, int status) => ResponseBody.fromString(
    body,
    status,
    headers: {
      Headers.contentTypeHeader: [Headers.jsonContentType],
    },
  );

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    requests.add(options);
    return handler(options);
  }

  @override
  void close({bool force = false}) {}
}

/// Provider overrides shared by the widget and controller tests.
List<Override> testOverrides({
  FakeApiClient? client,
  InMemorySessionStorage? storage,
  FakeDiscovery? discovery,
  WsClient? ws,

  /// Picks the client by agent address, for tests where addresses behave
  /// differently. By default every address gets [client].
  ApiClientFactory? clientFor,

  /// Search-result pictures. By default none load, and nothing is requested.
  ThumbnailLoader? thumbnailLoader,
}) {
  final api = client ?? FakeApiClient();
  return [
    apiClientFactoryProvider.overrideWithValue(
      clientFor ?? (agent, {token, onUnauthorized}) => api,
    ),
    sessionStorageProvider.overrideWithValue(
      storage ?? InMemorySessionStorage(),
    ),
    agentDiscoveryProvider.overrideWithValue(discovery ?? FakeDiscovery()),
    healthCheckIntervalProvider.overrideWithValue(null),
    thumbnailLoaderProvider.overrideWithValue(
      thumbnailLoader ?? FakeThumbnailLoader(),
    ),
    // No socket unless a test brings its own client.
    wsClientProvider.overrideWithValue(ws),
    deviceNameProvider.overrideWith((ref) async => 'Test Phone'),
  ];
}
