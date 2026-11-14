import 'dart:async';
import 'dart:convert';

import 'package:web_socket_channel/web_socket_channel.dart';

import 'models/ws_message.dart';

/// One open socket, as [WsClient] sees it. Tests use a fake.
abstract interface class WsConnection {
  /// Text frames. Completes or errors when the socket closes.
  Stream<String> get messages;

  void send(String text);

  Future<void> close();
}

/// Opens a connection, or throws if the agent can't be reached.
typedef WsConnector = Future<WsConnection> Function(Uri uri);

/// The real connector. The [uri] carries the token: never log it.
Future<WsConnection> connectWebSocket(Uri uri) async {
  final channel = WebSocketChannel.connect(uri);
  await channel.ready;
  return _ChannelConnection(channel);
}

class _ChannelConnection implements WsConnection {
  _ChannelConnection(this._channel);

  final WebSocketChannel _channel;

  @override
  Stream<String> get messages =>
      _channel.stream.where((frame) => frame is String).cast<String>();

  @override
  void send(String text) => _channel.sink.add(text);

  @override
  Future<void> close() => _channel.sink.close();
}

enum WsLinkStatus { connecting, connected, disconnected }

/// Keeps one WebSocket to the agent alive.
///
/// Sends `{"type":"ping"}` every [pingInterval]; no `pong` within
/// [pongTimeout] means the socket is dead and it reconnects. Reconnects back
/// off exponentially from [minBackoff] to [maxBackoff] and start over once the
/// agent has sent something. Unknown or malformed frames are ignored.
class WsClient {
  WsClient({
    required this.url,
    WsConnector? connector,
    this.pingInterval = const Duration(seconds: 15),
    this.pongTimeout = const Duration(seconds: 10),
    this.minBackoff = const Duration(seconds: 1),
    this.maxBackoff = const Duration(seconds: 30),
  }) : _connector = connector ?? connectWebSocket;

  /// Carries the token: never log it.
  final Uri url;
  final Duration pingInterval;
  final Duration pongTimeout;
  final Duration minBackoff;
  final Duration maxBackoff;
  final WsConnector _connector;

  final _events = StreamController<WsEvent>.broadcast();
  final _statuses = StreamController<WsLinkStatus>.broadcast();

  WsLinkStatus _status = WsLinkStatus.disconnected;
  bool _running = false;
  bool _disposed = false;
  int _failures = 0;

  /// Bumped whenever a connection ends, so late callbacks of an old
  /// connection are ignored.
  int _generation = 0;

  WsConnection? _connection;
  StreamSubscription<String>? _subscription;
  Timer? _pingTimer;
  Timer? _pongTimer;
  Timer? _retryTimer;

  Stream<WsEvent> get events => _events.stream;

  Stream<WsLinkStatus> get statuses => _statuses.stream;

  WsLinkStatus get status => _status;

  /// The delay before reconnect attempt number [failures] (1 = first retry).
  Duration backoffFor(int failures) {
    final factor = 1 << (failures - 1).clamp(0, 20);
    final delay = minBackoff * factor;
    return delay > maxBackoff ? maxBackoff : delay;
  }

  void start() {
    if (_running || _disposed) return;
    _running = true;
    _connect();
  }

  /// Stops and releases everything. The client can't be started again.
  Future<void> dispose() async {
    if (_disposed) return;
    _disposed = true;
    _running = false;
    _generation++;
    _cancelTimers();
    _setStatus(WsLinkStatus.disconnected);
    await _closeConnection();
    await _events.close();
    await _statuses.close();
  }

  Future<void> _connect() async {
    if (!_running) return;
    final generation = ++_generation;
    _setStatus(WsLinkStatus.connecting);
    final WsConnection connection;
    try {
      connection = await _connector(url);
    } catch (_) {
      if (generation == _generation) _connectionLost(generation);
      return;
    }
    if (!_running || generation != _generation) {
      await connection.close();
      return;
    }
    _connection = connection;
    _setStatus(WsLinkStatus.connected);
    var heardFromAgent = false;
    _subscription = connection.messages.listen(
      (text) {
        if (generation != _generation) return;
        if (!heardFromAgent) {
          heardFromAgent = true;
          _failures = 0;
        }
        _onFrame(text);
      },
      onError: (Object _) => _connectionLost(generation),
      onDone: () => _connectionLost(generation),
      cancelOnError: true,
    );
    _pingTimer = Timer.periodic(pingInterval, (_) => _ping(generation));
  }

  void _ping(int generation) {
    if (generation != _generation) return;
    try {
      _connection?.send(jsonEncode(const WsMessage(type: 'ping').toJson()));
    } catch (_) {
      _connectionLost(generation);
      return;
    }
    _pongTimer ??= Timer(pongTimeout, () => _connectionLost(generation));
  }

  void _onFrame(String text) {
    final WsEvent? event;
    try {
      final decoded = jsonDecode(text);
      if (decoded is! Map) return;
      event = parseWsEvent(
        WsMessage.fromJson(Map<String, Object?>.from(decoded)),
      );
    } catch (_) {
      return;
    }
    if (event == null) return;
    if (event is PongEvent) {
      _pongTimer?.cancel();
      _pongTimer = null;
      return;
    }
    if (!_events.isClosed) _events.add(event);
  }

  void _connectionLost(int generation) {
    if (generation != _generation || !_running) return;
    _generation++;
    _cancelTimers();
    _closeConnection();
    _setStatus(WsLinkStatus.disconnected);
    _failures++;
    _retryTimer = Timer(backoffFor(_failures), _connect);
  }

  Future<void> _closeConnection() async {
    final subscription = _subscription;
    final connection = _connection;
    _subscription = null;
    _connection = null;
    // Not awaited: closing the connection below is what matters, and waiting
    // for the cancel would only delay it.
    unawaited(subscription?.cancel());
    try {
      await connection?.close();
    } catch (_) {
      // Already closed: nothing to release.
    }
  }

  void _cancelTimers() {
    _pingTimer?.cancel();
    _pongTimer?.cancel();
    _retryTimer?.cancel();
    _pingTimer = null;
    _pongTimer = null;
    _retryTimer = null;
  }

  void _setStatus(WsLinkStatus status) {
    if (_status == status) return;
    _status = status;
    if (!_statuses.isClosed) _statuses.add(status);
  }
}
