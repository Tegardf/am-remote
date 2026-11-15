import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/queue_item.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/home/play_result.dart';
import 'package:am_remote/features/player/modes_controller.dart';
import 'package:am_remote/features/player/queue_controller.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';
import 'support/live_harness.dart';

const _notSupported = ApiException('NOT_SUPPORTED', 'cannot read it', 501);
const _notRunning = ApiException('APPLE_MUSIC_NOT_RUNNING', 'closed', 409);
const _noRoute = ApiException('NOT_FOUND', 'No such route', 404);

void main() {
  group('reading', () {
    test('asks the agent when it starts', () async {
      final h = LiveHarness();
      await h.start();

      expect(h.client.getQueueCalls, 1);
      expect(h.queue.status, QueueStatus.ready);
      expect(h.queue.items.map((i) => i.title), [
        'Queue Song 0',
        'Queue Song 1',
        'Auto Song 2',
      ]);
      await h.dispose();
    });

    test('is read again when the title changes, or the cover, but not for a '
        'new position', () async {
      final h = LiveHarness();
      await h.start();
      var reads = h.client.getQueueCalls;

      await h.push(wsStateFrame(title: 'Song A', artworkRev: 1));
      expect(h.client.getQueueCalls, reads + 1, reason: 'a new title');
      reads = h.client.getQueueCalls;

      // The same song, further on: nothing moved in the queue.
      await h.push(
        wsStateFrame(title: 'Song A', artworkRev: 1, positionMs: 90000),
      );
      expect(h.client.getQueueCalls, reads, reason: 'only the position');

      await h.push(wsStateFrame(title: 'Song A', artworkRev: 2));
      expect(h.client.getQueueCalls, reads + 1, reason: 'a new cover');
      reads = h.client.getQueueCalls;

      await h.push(wsStateFrame(title: 'Song B', artworkRev: 2));
      expect(h.client.getQueueCalls, reads + 1, reason: 'the next song');
      await h.dispose();
    });

    test('is read again after a shuffle or repeat change, not for the first '
        'modes', () async {
      final h = LiveHarness();
      await h.start();
      // `start` already read the modes (nothing → value): not a change.
      var reads = h.client.getQueueCalls;
      expect(reads, 1);

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'off'}));
      expect(h.client.getQueueCalls, reads + 1, reason: 'shuffle on');
      reads = h.client.getQueueCalls;

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'off'}));
      expect(h.client.getQueueCalls, reads, reason: 'the same modes again');

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'all'}));
      expect(h.client.getQueueCalls, reads + 1, reason: 'repeat changed');
      await h.dispose();
    });

    test('a change made from the phone reads it too', () async {
      final h = LiveHarness();
      await h.start();
      final reads = h.client.getQueueCalls;

      await h.container.read(modesControllerProvider.notifier).toggleShuffle();
      await pumpEventQueue();

      expect(h.client.getQueueCalls, reads + 1);
      await h.dispose();
    });

    test('is read again after a reconnect', () async {
      final h = LiveHarness(reconnectDelay: const Duration(milliseconds: 10));
      await h.start();
      final reads = h.client.getQueueCalls;

      await h.reconnect();

      expect(h.client.getQueueCalls, reads + 1);
      await h.dispose();
    });

    test('is never read on a timer', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final container = ProviderContainer(
          overrides: [
            ...testOverrides(client: client),
            apiClientProvider.overrideWithValue(client),
          ],
        );
        addTearDown(container.dispose);
        container.listen(queueControllerProvider, (_, _) {});
        async.flushMicrotasks();
        expect(client.getQueueCalls, 1);

        async.elapse(const Duration(minutes: 30));
        async.flushMicrotasks();

        expect(client.getQueueCalls, 1, reason: 'no polling');
      });
    });

    test('the latest read wins, even if an older one finishes last', () async {
      final slow = Completer<void>();
      final client = FakeApiClient()
        ..queue = const [QueueItem(index: 0, title: 'Old')]
        ..queueGate = slow;
      final h = LiveHarness(client: client);
      await h.start(); // the first read waits on its gate

      client
        ..queue = const [QueueItem(index: 0, title: 'New')]
        ..queueGate = null;
      await h.queueController.load();
      expect(h.queue.items.single.title, 'New');

      slow.complete(); // the older answer arrives last
      await pumpEventQueue();
      expect(h.queue.items.single.title, 'New');
      await h.dispose();
    });
  });

  group('failures', () {
    test('501, or an agent without the route (404), says it is not '
        'available', () async {
      for (final error in [_notSupported, _noRoute]) {
        final client = FakeApiClient()..queueError = error;
        final h = LiveHarness(client: client);
        await h.start();
        expect(h.queue.status, QueueStatus.notSupported);
        await h.dispose();
      }
    });

    test('409 means Apple Music is closed; reading again after it started '
        'recovers', () async {
      final client = FakeApiClient()..queueError = _notRunning;
      final h = LiveHarness(client: client);
      await h.start();
      expect(h.queue.status, QueueStatus.notRunning);

      client.queueError = null;
      await h.push(wsFrame('apple_music', {'running': true}));

      expect(h.queue.status, QueueStatus.ready);
      expect(h.queue.items, hasLength(3));
      await h.dispose();
    });

    test('a first read that fails is an error', () async {
      final client = FakeApiClient()
        ..queueError = const AgentUnreachableException();
      final h = LiveHarness(client: client);
      await h.start();

      expect(h.queue.status, QueueStatus.error);
      expect(h.queue.items, isEmpty);
      await h.dispose();
    });

    test('a failed refresh keeps the list that is shown, quietly', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      expect(h.queue.items, hasLength(3));

      client.queueError = const ApiException('INTERNAL', 'timeout', 500);
      await h.queueController.load();

      expect(h.queue.status, QueueStatus.ready);
      expect(h.queue.items, hasLength(3), reason: 'the list stays');
      await h.dispose();
    });

    test('an empty queue is ready with nothing in it', () async {
      final client = FakeApiClient()..queue = const [];
      final h = LiveHarness(client: client);
      await h.start();

      expect(h.queue.status, QueueStatus.ready);
      expect(h.queue.items, isEmpty);
      await h.dispose();
    });
  });

  group('playing a row', () {
    test('sends that row\'s index and title', () async {
      final h = LiveHarness();
      await h.start();

      final outcome = await h.queueController.play(h.queue.items[2]);

      expect(outcome, PlayResult.started);
      expect(h.client.playedQueue.single, (index: 2, title: 'Auto Song 2'));
      expect(h.queue.playingIndex, isNull);
      await h.dispose();
    });

    test('a second tap while one is on its way is ignored', () async {
      final client = FakeApiClient()..queuePlayGate = Completer<void>();
      final h = LiveHarness(client: client);
      await h.start();

      final first = h.queueController.play(h.queue.items[0]);
      await pumpEventQueue();
      expect(h.queue.playingIndex, 0);

      expect(await h.queueController.play(h.queue.items[1]), PlayResult.failed);
      expect(client.playedQueue, hasLength(1));

      client.queuePlayGate!.complete();
      expect(await first, PlayResult.started);
      expect(h.queue.playingIndex, isNull);
      await h.dispose();
    });

    test('404 (the queue moved on) reads the list again', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      final reads = client.getQueueCalls;

      client.queuePlayError = const ApiException('NOT_FOUND', 'moved', 404);
      client.queue = const [QueueItem(index: 0, title: 'Moved On')];
      final outcome = await h.queueController.play(h.queue.items[1]);
      await pumpEventQueue();

      expect(outcome, PlayResult.notFound);
      expect(client.getQueueCalls, reads + 1);
      expect(h.queue.items.single.title, 'Moved On');
      await h.dispose();
    });

    test('other failures say why', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      final row = h.queue.items.first;

      client.queuePlayError = _notSupported;
      expect(await h.queueController.play(row), PlayResult.notSupported);
      client.queuePlayError = _notRunning;
      expect(await h.queueController.play(row), PlayResult.notRunning);
      client.queuePlayError = const ApiException('INTERNAL', 'wrong song', 500);
      expect(await h.queueController.play(row), PlayResult.failed);
      expect(h.queueController.lastPlayFailure, 'wrong song');
      await h.dispose();
    });
  });

  test('a new pairing starts with a fresh queue', () async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    final client = FakeApiClient();
    final container = ProviderContainer(
      overrides: testOverrides(client: client, storage: storage),
    );
    addTearDown(container.dispose);
    container.listen(queueControllerProvider, (_, _) {});
    final session = container.read(sessionControllerProvider.notifier);
    await session.ready;
    await pumpEventQueue();
    expect(container.read(queueControllerProvider).items, isNotEmpty);

    await session.handleUnauthorized();
    await pumpEventQueue();
    client.queue = const [QueueItem(index: 0, title: 'Other Laptop')];
    await session.completePairing(testAgent, fakeToken);
    await pumpEventQueue();

    expect(container.read(queueControllerProvider).items.map((i) => i.title), [
      'Other Laptop',
    ]);
  });
}
