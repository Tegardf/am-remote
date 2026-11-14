import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/core/models/volume.dart';
import 'package:am_remote/core/models/ws_message.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

final _url = Uri.parse('ws://192.168.1.20:8765/api/v1/ws?token=$fakeToken');

WsClient _client(FakeWsConnector connector) =>
    WsClient(url: _url, connector: connector.call);

const _pong = '{"type":"pong"}';

void main() {
  test('the WebSocket URL is /api/v1/ws with the token as a query', () {
    final uri = testAgent.webSocketUri(fakeToken);
    expect(uri.scheme, 'ws');
    expect((uri.host, uri.port), ('192.168.1.20', 8765));
    expect(uri.path, '/api/v1/ws');
    expect(uri.queryParameters, {'token': fakeToken});
  });

  test('connects and turns frames into events, ignoring unknown ones', () {
    fakeAsync((async) {
      final connector = FakeWsConnector();
      final client = _client(connector);
      final events = <WsEvent>[];
      client.events.listen(events.add);

      client.start();
      async.flushMicrotasks();
      expect(client.status, WsLinkStatus.connected);

      final agent = connector.last;
      // The agent's connect order: state, apple_music, then volume.
      agent.serverSend(wsStateFrame());
      agent.serverSend(wsFrame('apple_music', {'running': false}));
      agent.serverSend(wsFrame('volume', {'level': 42, 'muted': false}));
      // Not understood: must be ignored, and must not break the connection.
      agent.serverSend(wsFrame('from_the_future', {'x': 1}));
      agent.serverSend('not json at all');
      agent.serverSend('[1,2,3]');
      agent.serverSend(wsFrame('state', {'status': 'nonsense'}));
      async.flushMicrotasks();

      expect(events.length, 4);
      expect((events[0] as StateEvent).state.title, 'Example Song');
      expect((events[1] as AppleMusicEvent).running, isFalse);
      expect(
        (events[2] as VolumeEvent).volume,
        const Volume(level: 42, muted: false),
      );
      // An unknown status is still a state, with status "unknown".
      expect((events[3] as StateEvent).state.status, PlaybackStatus.unknown);
      expect(client.status, WsLinkStatus.connected);
      expect(connector.attempts, 1);

      client.dispose();
    });
  });

  test('sends a ping every 15 s and stays connected while pongs arrive', () {
    fakeAsync((async) {
      final connector = FakeWsConnector();
      final client = _client(connector);
      client.start();
      async.flushMicrotasks();
      final agent = connector.last;

      async.elapse(const Duration(seconds: 14));
      expect(agent.pings, 0);
      async.elapse(const Duration(seconds: 1));
      expect(agent.pings, 1);

      for (var i = 0; i < 4; i++) {
        agent.serverSend(_pong);
        async.elapse(const Duration(seconds: 15));
      }
      expect(agent.pings, 5);
      expect(connector.attempts, 1, reason: 'no reconnect while pongs arrive');
      expect(agent.closed, isFalse);

      client.dispose();
    });
  });

  test('a missing pong reconnects after the 10 s timeout and 1 s backoff', () {
    fakeAsync((async) {
      final connector = FakeWsConnector();
      final client = _client(connector);
      client.start();
      async.flushMicrotasks();
      final first = connector.last;

      async.elapse(const Duration(seconds: 15)); // ping sent, no pong
      async.elapse(const Duration(seconds: 9));
      expect(first.closed, isFalse);
      async.elapse(const Duration(seconds: 1)); // 10 s without a pong
      async.flushMicrotasks();
      expect(first.closed, isTrue);
      expect(client.status, WsLinkStatus.disconnected);
      expect(connector.attempts, 1);

      async.elapse(const Duration(seconds: 1)); // backoff
      async.flushMicrotasks();
      expect(connector.attempts, 2);
      expect(client.status, WsLinkStatus.connected);

      client.dispose();
    });
  });

  test('a dropped connection reconnects', () {
    fakeAsync((async) {
      final connector = FakeWsConnector();
      final client = _client(connector);
      client.start();
      async.flushMicrotasks();

      connector.last.serverDrop();
      async.flushMicrotasks();
      expect(client.status, WsLinkStatus.disconnected);

      async.elapse(const Duration(seconds: 1));
      async.flushMicrotasks();
      expect(connector.attempts, 2);
      expect(client.status, WsLinkStatus.connected);

      client.dispose();
    });
  });

  test('backoff doubles from 1 s and stops at 30 s', () {
    final client = _client(FakeWsConnector());
    expect(
      [for (var n = 1; n <= 8; n++) client.backoffFor(n).inSeconds],
      [1, 2, 4, 8, 16, 30, 30, 30],
    );
  });

  test('refused connections are retried with growing delays', () {
    fakeAsync((async) {
      final connector = FakeWsConnector(failFirst: 100);
      final client = _client(connector);
      client.start();
      async.flushMicrotasks();
      expect(connector.attempts, 1);

      // Retries come 1 s, 2 s, 4 s and 8 s after each failure.
      for (final (wait, attempts) in [(1, 2), (2, 3), (4, 4), (8, 5)]) {
        async.elapse(Duration(seconds: wait - 1));
        expect(connector.attempts, attempts - 1, reason: 'before ${wait}s');
        async.elapse(const Duration(seconds: 1));
        async.flushMicrotasks();
        expect(connector.attempts, attempts, reason: 'after ${wait}s');
      }

      client.dispose();
    });
  });

  test('the backoff starts over once the agent has sent something', () {
    fakeAsync((async) {
      final connector = FakeWsConnector(failFirst: 2);
      final client = _client(connector);
      client.start();
      async.flushMicrotasks(); // attempt 1 refused
      async.elapse(const Duration(seconds: 1)); // attempt 2 refused
      async.flushMicrotasks();
      async.elapse(const Duration(seconds: 2)); // attempt 3 connects
      async.flushMicrotasks();
      expect(connector.attempts, 3);

      connector.last.serverSend(wsStateFrame());
      async.flushMicrotasks();
      connector.last.serverDrop();
      async.flushMicrotasks();

      // 1 s again, not 4 s.
      async.elapse(const Duration(seconds: 1));
      async.flushMicrotasks();
      expect(connector.attempts, 4);

      client.dispose();
    });
  });

  test('dispose closes the connection and stops all retries', () {
    fakeAsync((async) {
      final connector = FakeWsConnector();
      final client = _client(connector);
      client.start();
      async.flushMicrotasks();
      final agent = connector.last;

      client.dispose();
      async.flushMicrotasks();
      expect(agent.closed, isTrue);

      async.elapse(const Duration(minutes: 2));
      async.flushMicrotasks();
      expect(connector.attempts, 1);
      expect(agent.pings, 0);
      expect(client.status, WsLinkStatus.disconnected);
    });
  });
}
