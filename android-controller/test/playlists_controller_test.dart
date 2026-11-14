import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/playlist.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/home/play_result.dart';
import 'package:am_remote/features/playlists/playlists_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

ProviderContainer _container(FakeApiClient client) {
  final container = ProviderContainer(
    overrides: [
      ...testOverrides(client: client),
      apiClientProvider.overrideWithValue(client),
    ],
  );
  addTearDown(container.dispose);
  container.listen(playlistsControllerProvider, (_, _) {});
  return container;
}

PlaylistsState _state(ProviderContainer c) =>
    c.read(playlistsControllerProvider);

PlaylistsController _controller(ProviderContainer c) =>
    c.read(playlistsControllerProvider.notifier);

const _a = [Playlist(id: 'a', name: 'List A')];
const _b = [Playlist(id: 'b', name: 'List B')];

void main() {
  test('reads the playlists when first used', () async {
    final client = FakeApiClient();
    final c = _container(client);
    expect(_state(c).status, PlaylistsStatus.loading);

    await pumpEventQueue();

    expect(_state(c).status, PlaylistsStatus.ready);
    expect(_state(c).items.map((p) => p.id), ['pl-1', 'pl-2']);
    expect(client.getPlaylistsCalls, 1);
  });

  test('the latest load wins, even if an older one finishes last', () async {
    final client = FakeApiClient();
    final slowFirst = Completer<void>();
    final fastSecond = Completer<void>();

    client.playlists = _a;
    client.playlistsGate = slowFirst;
    final c = _container(client);
    await pumpEventQueue(); // the first load is waiting on its gate

    client.playlists = _b;
    client.playlistsGate = fastSecond;
    final second = _controller(c).load();
    await pumpEventQueue();

    fastSecond.complete();
    await second;
    expect(_state(c).items, _b);

    slowFirst.complete(); // the older answer arrives last
    await pumpEventQueue();
    expect(_state(c).items, _b, reason: 'the old answer must not win');
  });

  test('a failed first read shows the error state', () async {
    final client = FakeApiClient()
      ..playlistsError = const AgentUnreachableException();
    final c = _container(client);
    await pumpEventQueue();

    expect(_state(c).status, PlaylistsStatus.error);
    expect(_state(c).items, isEmpty);
  });

  test(
    'NOT_SUPPORTED and APPLE_MUSIC_NOT_RUNNING get their own states',
    () async {
      final client = FakeApiClient()
        ..playlistsError = const ApiException('NOT_SUPPORTED', 'no', 501);
      final c = _container(client);
      await pumpEventQueue();
      expect(_state(c).status, PlaylistsStatus.notSupported);

      client.playlistsError = const ApiException(
        'APPLE_MUSIC_NOT_RUNNING',
        'closed',
        409,
      );
      await _controller(c).load();
      expect(_state(c).status, PlaylistsStatus.notRunning);
    },
  );

  test('a failed refresh keeps the list and says so once', () async {
    final client = FakeApiClient();
    final c = _container(client);
    await pumpEventQueue();
    expect(_state(c).items, hasLength(2));
    expect(_state(c).refreshErrorId, 0);

    client.playlistsError = const AgentUnreachableException();
    await _controller(c).load();

    expect(_state(c).status, PlaylistsStatus.ready);
    expect(_state(c).items, hasLength(2), reason: 'the list stays');
    expect(_state(c).refreshErrorId, 1);

    await _controller(c).load();
    expect(_state(c).refreshErrorId, 2);
  });

  test('a successful refresh replaces the list', () async {
    final client = FakeApiClient();
    final c = _container(client);
    await pumpEventQueue();

    client.playlists = _b;
    await _controller(c).load();

    expect(_state(c).items, _b);
  });

  test(
    'playing a playlist sends its id, and a second tap meanwhile is ignored',
    () async {
      final client = FakeApiClient()..playGate = Completer<void>();
      final c = _container(client);
      await pumpEventQueue();

      final first = _controller(c).play('pl-1');
      await pumpEventQueue();
      expect(_state(c).playingId, 'pl-1');

      expect(await _controller(c).play('pl-2'), PlayResult.failed);
      expect(client.playedPlaylists, ['pl-1']);

      client.playGate!.complete();
      expect(await first, PlayResult.started);
      expect(_state(c).playingId, isNull);
    },
  );

  test('a play that fails reports why', () async {
    final client = FakeApiClient();
    final c = _container(client);
    await pumpEventQueue();

    client.playError = const ApiException('NOT_SUPPORTED', 'no', 501);
    expect(await _controller(c).play('pl-1'), PlayResult.notSupported);
    client.playError = const ApiException('APPLE_MUSIC_NOT_RUNNING', 'x', 409);
    expect(await _controller(c).play('pl-1'), PlayResult.notRunning);
    client.playError = const AgentUnreachableException();
    expect(await _controller(c).play('pl-1'), PlayResult.failed);
  });

  test('reads the list again when the socket reconnects, not on the first '
      'connection (ids may change when Apple Music restarts)', () async {
    final connector = FakeWsConnector();
    final ws = WsClient(
      url: testAgent.webSocketUri(fakeToken),
      connector: connector.call,
      minBackoff: const Duration(milliseconds: 10),
      maxBackoff: const Duration(milliseconds: 20),
    );
    final client = FakeApiClient();
    final container = ProviderContainer(
      overrides: testOverrides(
        client: client,
        ws: ws,
        storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
      ),
    );
    addTearDown(() async {
      container.dispose();
      await ws.dispose();
    });
    container.listen(playlistsControllerProvider, (_, _) {});
    await container.read(sessionControllerProvider.notifier).ready;

    ws.start();
    await pumpEventQueue();
    expect(connector.connections, hasLength(1));
    expect(client.getPlaylistsCalls, 1, reason: 'the first connection');

    // Apple Music restarted: new ids on the laptop.
    client.playlists = _b;
    connector.last.serverDrop();
    await Future<void>.delayed(const Duration(milliseconds: 120));
    await pumpEventQueue();

    expect(connector.connections, hasLength(2), reason: 'it reconnected');
    expect(client.getPlaylistsCalls, 2);
    expect(_state(container).items, _b);
  });

  test('a playlist that is gone (NOT_FOUND) refreshes the list', () async {
    final client = FakeApiClient();
    final c = _container(client);
    await pumpEventQueue();
    expect(_state(c).items, hasLength(2));
    expect(client.getPlaylistsCalls, 1);

    // It was deleted on the laptop meanwhile.
    client.playlists = _b;
    client.playError = const ApiException('NOT_FOUND', 'no such id', 404);
    expect(await _controller(c).play('pl-1'), PlayResult.notFound);
    await pumpEventQueue();

    expect(client.getPlaylistsCalls, 2);
    expect(_state(c).items, _b);
  });

  test(
    'reads again by itself when the connection comes back after an error',
    () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final client = FakeApiClient()
        ..playlistsError = const AgentUnreachableException();
      final container = ProviderContainer(
        overrides: testOverrides(client: client, storage: storage),
      );
      addTearDown(container.dispose);
      container.listen(playlistsControllerProvider, (_, _) {});
      await container.read(sessionControllerProvider.notifier).ready;
      await pumpEventQueue();
      expect(_state(container).status, PlaylistsStatus.error);

      // Offline for a while ...
      client.statusError = const AgentUnreachableException();
      await container
          .read(sessionControllerProvider.notifier)
          .checkConnection();
      await pumpEventQueue();
      expect(_state(container).status, PlaylistsStatus.error);

      // ... then the laptop is back: no tap needed.
      client.statusError = null;
      client.playlistsError = null;
      await container
          .read(sessionControllerProvider.notifier)
          .checkConnection();
      await pumpEventQueue();

      expect(_state(container).status, PlaylistsStatus.ready);
      expect(_state(container).items, hasLength(2));
    },
  );
}
