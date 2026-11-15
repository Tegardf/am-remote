import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/player/live_controller.dart';
import 'package:am_remote/features/player/modes_controller.dart';
import 'package:am_remote/features/player/queue_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fakes.dart';

/// A fake agent on a fake socket, with the live, modes and queue controllers
/// kept alive. The agent "pushes" frames with [push].
class LiveHarness {
  LiveHarness({FakeApiClient? client, Duration? reconnectDelay})
    : client = client ?? FakeApiClient() {
    ws = WsClient(
      url: testAgent.webSocketUri(fakeToken),
      connector: connector.call,
      minBackoff: reconnectDelay ?? const Duration(seconds: 1),
      maxBackoff: reconnectDelay ?? const Duration(seconds: 30),
    );
    container = ProviderContainer(
      overrides: [
        ...testOverrides(client: this.client, ws: ws),
        apiClientProvider.overrideWithValue(this.client),
      ],
    );
    container
      ..listen(liveControllerProvider, (_, _) {})
      ..listen(modesControllerProvider, (_, _) {})
      ..listen(queueControllerProvider, (_, _) {});
  }

  final FakeApiClient client;
  final connector = FakeWsConnector();
  late final WsClient ws;
  late final ProviderContainer container;

  LiveState get live => container.read(liveControllerProvider);
  ModesState get modes => container.read(modesControllerProvider);
  QueueState get queue => container.read(queueControllerProvider);
  ModesController get modesController =>
      container.read(modesControllerProvider.notifier);
  QueueController get queueController =>
      container.read(queueControllerProvider.notifier);

  /// Connects and lets the first reads finish.
  Future<void> start() async {
    ws.start();
    await pumpEventQueue();
  }

  Future<void> push(String frame) async {
    connector.last.serverSend(frame);
    await pumpEventQueue();
  }

  /// The agent drops the socket and the app connects again (needs a short
  /// `reconnectDelay`).
  Future<void> reconnect() async {
    final before = connector.connections.length;
    connector.last.serverDrop();
    for (var i = 0; i < 50 && connector.connections.length == before; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    await pumpEventQueue();
    expect(connector.connections.length, before + 1, reason: 'reconnected');
  }

  Future<void> dispose() async {
    container.dispose();
    await ws.dispose();
  }
}
