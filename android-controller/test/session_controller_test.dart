import 'package:am_remote/core/agent_address.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/discovery.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

ProviderContainer _container({
  required FakeApiClient client,
  required InMemorySessionStorage storage,
  FakeDiscovery? discovery,
}) {
  final container = ProviderContainer(
    overrides: testOverrides(
      client: client,
      storage: storage,
      discovery: discovery,
    ),
  );
  addTearDown(container.dispose);
  return container;
}

Future<SessionState> _start(ProviderContainer container) async {
  container.read(sessionControllerProvider);
  await container.read(sessionControllerProvider.notifier).ready;
  return container.read(sessionControllerProvider);
}

const _unauthorized = ApiException(
  'UNAUTHORIZED',
  'Missing or invalid token',
  401,
);

void main() {
  test('no saved token → pairing', () async {
    final container = _container(
      client: FakeApiClient(),
      storage: InMemorySessionStorage(),
    );
    expect(await _start(container), isA<SessionUnpaired>());
  });

  test('saved token + 200 → paired and connected', () async {
    final client = FakeApiClient();
    final container = _container(
      client: client,
      storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
    );

    final state = await _start(container) as SessionPaired;
    expect(state.connection, ConnectionStatus.connected);
    expect(state.agent, testAgent);
    expect(state.status?.appleMusicRunning, isTrue);
    expect(client.statusCalls, 1);
  });

  test('saved token + 401 → pairing, with the token cleared', () async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    final container = _container(
      client: FakeApiClient()..statusError = _unauthorized,
      storage: storage,
    );

    expect(await _start(container), isA<SessionUnpaired>());
    expect(storage.token, isNull);
  });

  test('agent unreachable → paired but offline, token kept', () async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    final container = _container(
      client: FakeApiClient()..statusError = const AgentUnreachableException(),
      storage: storage,
    );

    final state = await _start(container) as SessionPaired;
    expect(state.connection, ConnectionStatus.offline);
    expect(storage.token, fakeToken);
  });

  test('offline → connected once the agent answers again', () async {
    final client = FakeApiClient()
      ..statusError = const AgentUnreachableException();
    final container = _container(
      client: client,
      storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
    );
    await _start(container);

    client.statusError = null;
    await container.read(sessionControllerProvider.notifier).checkConnection();

    final state = container.read(sessionControllerProvider) as SessionPaired;
    expect(state.connection, ConnectionStatus.connected);
  });

  test('completePairing stores token and host and opens the app', () async {
    final storage = InMemorySessionStorage();
    final container = _container(client: FakeApiClient(), storage: storage);
    await _start(container);

    await container
        .read(sessionControllerProvider.notifier)
        .completePairing(testAgent, fakeToken);

    expect(storage.token, fakeToken);
    expect(storage.agent, testAgent);
    expect(container.read(sessionControllerProvider), isA<SessionPaired>());
  });

  test(
    'handleUnauthorized (e.g. a later 401) clears token and pairs again',
    () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final container = _container(client: FakeApiClient(), storage: storage);
      await _start(container);

      await container
          .read(sessionControllerProvider.notifier)
          .handleUnauthorized();

      expect(container.read(sessionControllerProvider), isA<SessionUnpaired>());
      expect(storage.token, isNull);
    },
  );

  group('re-discovery when the saved host keeps failing', () {
    const moved = DiscoveredAgent(
      name: 'LAPTOP',
      host: '192.168.1.99',
      port: 8765,
      version: '1',
    );

    Future<void> failTimes(ProviderContainer container, int times) async {
      final controller = container.read(sessionControllerProvider.notifier);
      for (var i = 0; i < times; i++) {
        await controller.checkConnection();
      }
      await pumpEventQueue();
    }

    test('follows the laptop to its new address after 3 failures', () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final discovery = FakeDiscovery(const [moved]);
      // The old address is down; the new one answers.
      final oldAddress = FakeApiClient()
        ..statusError = const AgentUnreachableException();
      final newAddress = FakeApiClient();
      final container = ProviderContainer(
        overrides: testOverrides(
          storage: storage,
          discovery: discovery,
          clientFor: (agent, {token, onUnauthorized}) =>
              agent.host == '192.168.1.99' ? newAddress : oldAddress,
        ),
      );
      addTearDown(container.dispose);

      await _start(container); // failure 1
      await failTimes(container, 1); // failure 2
      expect(discovery.searches, 0, reason: 'not yet');

      await failTimes(container, 1); // failure 3 → looks again, finds it

      final state = container.read(sessionControllerProvider) as SessionPaired;
      expect(discovery.searches, 1);
      expect(state.agent.host, '192.168.1.99');
      expect(storage.agent?.host, '192.168.1.99');
      expect(storage.token, fakeToken, reason: 'same pairing, new address');
      expect(state.connection, ConnectionStatus.connected);
    });

    group('the TXT ip can be stale (contract C-004)', () {
      /// A fake network: only the hosts in [alive] answer. [asked] records
      /// every host a client was made for, probes included.
      ProviderContainer network({
        required InMemorySessionStorage storage,
        required FakeDiscovery discovery,
        required Map<String, FakeApiClient> alive,
        required List<String> asked,
      }) {
        final down = FakeApiClient()
          ..statusError = const AgentUnreachableException();
        final container = ProviderContainer(
          overrides: testOverrides(
            storage: storage,
            discovery: discovery,
            clientFor: (agent, {token, onUnauthorized}) {
              asked.add(agent.host);
              return alive[agent.host] ?? down;
            },
          ),
        );
        addTearDown(container.dispose);
        return container;
      }

      test('falls back to the address the resolver gave', () async {
        // The laptop moved from .20 to .99, but its TXT ip still says .20.
        final storage = InMemorySessionStorage(
          token: fakeToken,
          agent: testAgent,
        );
        final discovery = FakeDiscovery(const [
          DiscoveredAgent(
            name: 'LAPTOP',
            host: '192.168.1.20',
            port: 8765,
            alternativeHosts: ['192.168.1.99'],
          ),
        ]);
        final container = network(
          storage: storage,
          discovery: discovery,
          alive: {'192.168.1.99': FakeApiClient()},
          asked: [],
        );

        await _start(container); // failure 1
        await failTimes(container, 2); // failures 2 and 3 → looks again

        final state =
            container.read(sessionControllerProvider) as SessionPaired;
        expect(discovery.searches, 1);
        expect(state.agent.host, '192.168.1.99');
        expect(storage.agent?.host, '192.168.1.99');
        expect(state.connection, ConnectionStatus.connected);
      });

      test(
        'tries the addresses in order and takes the first that answers',
        () async {
          final storage = InMemorySessionStorage(
            token: fakeToken,
            agent: testAgent,
          );
          final asked = <String>[];
          final container = network(
            storage: storage,
            discovery: FakeDiscovery(const [
              DiscoveredAgent(
                name: 'LAPTOP',
                host: '192.168.1.50', // out of date too
                port: 8765,
                alternativeHosts: ['192.168.1.99', '192.168.1.77'],
              ),
            ]),
            alive: {
              '192.168.1.99': FakeApiClient(),
              '192.168.1.77': FakeApiClient(),
            },
            asked: asked,
          );

          await _start(container);
          await failTimes(container, 2);

          final state =
              container.read(sessionControllerProvider) as SessionPaired;
          expect(state.agent.host, '192.168.1.99');
          expect(asked, contains('192.168.1.50'), reason: 'tried first');
          expect(asked, isNot(contains('192.168.1.77')), reason: 'not needed');
        },
      );

      test('never moves to an address that does not answer; the next round '
          'tries again', () async {
        final storage = InMemorySessionStorage(
          token: fakeToken,
          agent: testAgent,
        );
        final discovery = FakeDiscovery(const [
          DiscoveredAgent(
            name: 'LAPTOP',
            host: '192.168.1.50',
            port: 8765,
            alternativeHosts: ['192.168.1.99'],
          ),
        ]);
        final later = FakeApiClient()
          ..statusError = const AgentUnreachableException();
        final container = network(
          storage: storage,
          discovery: discovery,
          alive: {'192.168.1.99': later},
          asked: [],
        );

        await _start(container);
        await failTimes(container, 2); // round 1: nobody answers
        var state = container.read(sessionControllerProvider) as SessionPaired;
        expect(discovery.searches, 1);
        expect(state.agent.host, '192.168.1.20', reason: 'stays put');
        expect(storage.agent?.host, '192.168.1.20');

        later.statusError = null; // the laptop is reachable at .99 now
        await failTimes(container, 3); // round 2

        state = container.read(sessionControllerProvider) as SessionPaired;
        expect(discovery.searches, 2);
        expect(state.agent.host, '192.168.1.99');
        expect(state.connection, ConnectionStatus.connected);
      });

      test('an address that answers with an error is still the laptop; a 401 '
          'there is handled by the normal check', () async {
        final storage = InMemorySessionStorage(
          token: fakeToken,
          agent: testAgent,
        );
        final revoked = FakeApiClient()..statusError = _unauthorized;
        final container = network(
          storage: storage,
          discovery: FakeDiscovery(const [
            DiscoveredAgent(name: 'LAPTOP', host: '192.168.1.99', port: 8765),
          ]),
          alive: {'192.168.1.99': revoked},
          asked: [],
        );

        await _start(container);
        await failTimes(container, 2);

        // Followed to .99, where the check found the token revoked.
        expect(
          container.read(sessionControllerProvider),
          isA<SessionUnpaired>(),
        );
        expect(storage.token, isNull);
      });
    });

    test('does nothing before the third failure', () async {
      final discovery = FakeDiscovery(const [moved]);
      final container = _container(
        client: FakeApiClient()
          ..statusError = const AgentUnreachableException(),
        storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
        discovery: discovery,
      );
      await _start(container); // failure 1
      await failTimes(container, 1); // failure 2

      expect(discovery.searches, 0);
      final state = container.read(sessionControllerProvider) as SessionPaired;
      expect(state.agent.host, '192.168.1.20');
    });

    test('a success in between starts the count over', () async {
      final discovery = FakeDiscovery(const [moved]);
      final client = FakeApiClient()
        ..statusError = const AgentUnreachableException();
      final container = _container(
        client: client,
        storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
        discovery: discovery,
      );
      await _start(container); // failure 1
      await failTimes(container, 1); // failure 2

      client.statusError = null;
      await failTimes(container, 1); // success: back to zero
      client.statusError = const AgentUnreachableException();
      await failTimes(container, 2); // failures 1 and 2 again

      expect(discovery.searches, 0);
    });

    test('ignores agents with another name', () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final discovery = FakeDiscovery(const [
        DiscoveredAgent(name: 'SOMEONE-ELSE', host: '192.168.1.50', port: 8765),
      ]);
      final container = _container(
        client: FakeApiClient()
          ..statusError = const AgentUnreachableException(),
        storage: storage,
        discovery: discovery,
      );
      await _start(container);
      await failTimes(container, 2);

      expect(discovery.searches, 1);
      final state = container.read(sessionControllerProvider) as SessionPaired;
      expect(state.agent.host, '192.168.1.20');
      expect(storage.agent?.host, '192.168.1.20');
    });

    test(
      'a manually typed address has no name, so it is never replaced',
      () async {
        final discovery = FakeDiscovery(const [moved]);
        final container = _container(
          client: FakeApiClient()
            ..statusError = const AgentUnreachableException(),
          storage: InMemorySessionStorage(
            token: fakeToken,
            agent: const AgentAddress(host: '10.0.2.2'),
          ),
          discovery: discovery,
        );
        await _start(container);
        await failTimes(container, 5);

        expect(discovery.searches, 0);
        final state =
            container.read(sessionControllerProvider) as SessionPaired;
        expect(state.agent.host, '10.0.2.2');
      },
    );

    test('keeps the address when discovery finds the same one', () async {
      final storage = InMemorySessionStorage(
        token: fakeToken,
        agent: testAgent,
      );
      final container = _container(
        client: FakeApiClient()
          ..statusError = const AgentUnreachableException(),
        storage: storage,
        discovery: FakeDiscovery(const [
          DiscoveredAgent(name: 'LAPTOP', host: '192.168.1.20', port: 8765),
        ]),
      );
      await _start(container);
      await failTimes(container, 2);

      final state = container.read(sessionControllerProvider) as SessionPaired;
      expect(state.connection, ConnectionStatus.offline);
      expect(state.agent.host, '192.168.1.20');
    });
  });
}
