import 'package:am_remote/core/agent_address.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/home/tabs.dart';
import 'package:am_remote/features/playlists/playlists_controller.dart';
import 'package:am_remote/features/search/search_controller.dart';
import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

const _otherLaptop = AgentAddress(host: '192.168.1.77', name: 'OTHER-LAPTOP');

void main() {
  test('after unpairing and pairing again, Playlists, Search and the tab start fresh', () async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    final client = FakeApiClient();
    final container = ProviderContainer(
      overrides: testOverrides(client: client, storage: storage),
    );
    addTearDown(container.dispose);
    container
      ..listen(playlistsControllerProvider, (_, _) {})
      ..listen(searchControllerProvider, (_, _) {})
      ..listen(selectedTabProvider, (_, _) {});
    final session = container.read(sessionControllerProvider.notifier);
    await session.ready;
    await pumpEventQueue();

    // Use the app: a playlist list, a search with results, the Search tab.
    expect(container.read(playlistsControllerProvider).items, isNotEmpty);
    container.read(searchControllerProvider.notifier)
      ..setQuery('example')
      ..submit();
    await pumpEventQueue();
    container.read(selectedTabProvider.notifier).select(AppTab.search);
    expect(container.read(searchControllerProvider).results, isNotEmpty);
    final loadsBefore = client.getPlaylistsCalls;

    // A 401 sends the app back to pairing.
    await session.handleUnauthorized();
    await pumpEventQueue();
    expect(container.read(sessionControllerProvider), isA<SessionUnpaired>());

    // Pair again, with another laptop this time.
    client.playlists = const [];
    await session.completePairing(_otherLaptop, fakeToken);
    await pumpEventQueue();

    final search = container.read(searchControllerProvider);
    expect(search.query, '');
    expect(search.status, SearchStatus.idle);
    expect(search.results, isEmpty);
    expect(container.read(selectedTabProvider), AppTab.player);
    // The playlists are read again, from the new laptop, not left over.
    expect(client.getPlaylistsCalls, greaterThan(loadsBefore));
    final playlists = container.read(playlistsControllerProvider);
    expect(playlists.status, PlaylistsStatus.ready);
    expect(playlists.items, isEmpty);
  });

  test(
    'a search that failed with the 401 does not leave its error behind',
    () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final client = FakeApiClient();
      final container = ProviderContainer(
        overrides: testOverrides(client: client, storage: storage),
      );
      addTearDown(container.dispose);
      container.listen(searchControllerProvider, (_, _) {});
      final session = container.read(sessionControllerProvider.notifier);
      await session.ready;

      client.searchError = const ApiException('UNAUTHORIZED', 'revoked', 401);
      container.read(searchControllerProvider.notifier)
        ..setQuery('example')
        ..submit();
      await pumpEventQueue();
      expect(
        container.read(searchControllerProvider).status,
        SearchStatus.error,
      );

      await session.handleUnauthorized();
      await pumpEventQueue(); // the app shows the connect screen meanwhile
      client.searchError = null;
      await session.completePairing(testAgent, fakeToken);
      await pumpEventQueue();

      expect(
        container.read(searchControllerProvider).status,
        SearchStatus.idle,
      );
    },
  );

  group('a real 401 on a new call sends the app back to pairing', () {
    /// A real client over a fake network: `/status` is fine, all else is 401.
    ProviderContainer revokedAfterPairing(InMemorySessionStorage storage) {
      final container = ProviderContainer(
        overrides: testOverrides(
          storage: storage,
          clientFor: (agent, {token, onUnauthorized}) => DioApiClient(
            agent: agent,
            token: token,
            onUnauthorized: onUnauthorized,
            dio: Dio()
              ..httpClientAdapter = FakeHttpAdapter(
                (o) => o.uri.path.endsWith('/status')
                    ? FakeHttpAdapter.json(
                        '{"agent_version":"0.1.0","apple_music_running":true}',
                        200,
                      )
                    : FakeHttpAdapter.json(
                        '{"error":{"code":"UNAUTHORIZED","message":"revoked"}}',
                        401,
                      ),
              ),
          ),
        ),
      );
      addTearDown(container.dispose);
      return container;
    }

    test('GET /playlists', () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final container = revokedAfterPairing(storage);
      await container.read(sessionControllerProvider.notifier).ready;
      expect(container.read(sessionControllerProvider), isA<SessionPaired>());

      container.listen(playlistsControllerProvider, (_, _) {});
      await pumpEventQueue();

      expect(container.read(sessionControllerProvider), isA<SessionUnpaired>());
      expect(storage.token, isNull);
    });

    test('GET /search', () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final container = revokedAfterPairing(storage);
      await container.read(sessionControllerProvider.notifier).ready;
      container.listen(searchControllerProvider, (_, _) {});

      container.read(searchControllerProvider.notifier)
        ..setQuery('example')
        ..submit();
      await pumpEventQueue();

      expect(container.read(sessionControllerProvider), isA<SessionUnpaired>());
      expect(storage.token, isNull);
    });
  });
}
