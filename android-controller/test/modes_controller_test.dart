import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/modes.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';
import 'support/live_harness.dart';

const _notSupported = ApiException('NOT_SUPPORTED', 'no buttons', 501);
const _notRunning = ApiException('APPLE_MUSIC_NOT_RUNNING', 'closed', 409);
const _noRoute = ApiException('NOT_FOUND', 'No such route', 404);

/// An agent that answers a change with what Apple Music shows afterwards, and
/// Apple Music did not do it.
class _StubbornClient extends FakeApiClient {
  @override
  Future<Modes> setModes({bool? shuffle, RepeatSetting? repeat}) async {
    setModesCalls.add((shuffle: shuffle, repeat: repeat));
    return modes;
  }
}

void main() {
  group('reading', () {
    test('asks the agent when it starts', () async {
      final client = FakeApiClient()
        ..modes = const Modes(shuffle: true, repeat: RepeatSetting.all);
      final h = LiveHarness(client: client);
      await h.start();

      expect(client.getModesCalls, 1);
      expect(
        h.modes.modes,
        const Modes(shuffle: true, repeat: RepeatSetting.all),
      );
      expect(h.modes.usable, isTrue);
      await h.dispose();
    });

    test('a pushed modes frame updates the buttons', () async {
      final h = LiveHarness();
      await h.start();

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'one'}));

      expect(
        h.modes.modes,
        const Modes(shuffle: true, repeat: RepeatSetting.one),
      );
      await h.dispose();
    });

    test('an unknown repeat value becomes off', () async {
      final h = LiveHarness();
      await h.start();

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'sometimes'}));

      expect(h.modes.modes?.repeat, RepeatSetting.off);
      expect(h.modes.modes?.shuffle, isTrue);
      await h.dispose();
    });

    test('a push is newer than a read that was still on its way', () async {
      final client = FakeApiClient()..modesGate = Completer<void>();
      final h = LiveHarness(client: client);
      await h.start(); // the first read waits on its gate

      await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'off'}));
      client.modesGate!.complete(); // the old answer (shuffle off) arrives
      await pumpEventQueue();

      expect(h.modes.modes?.shuffle, isTrue, reason: 'the old answer loses');
      await h.dispose();
    });

    test('the first connection does not read again', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      await h.push(wsFrame('state', {'status': 'playing'}));

      expect(client.getModesCalls, 1);
      await h.dispose();
    });

    test('a reconnect reads again', () async {
      final client = FakeApiClient();
      final h = LiveHarness(
        client: client,
        reconnectDelay: const Duration(milliseconds: 10),
      );
      await h.start();
      await h.push(wsFrame('modes', {'shuffle': false, 'repeat': 'off'}));
      expect(client.getModesCalls, 1);

      // The laptop's buttons were changed while the socket was down.
      client.modes = const Modes(shuffle: true, repeat: RepeatSetting.one);
      await h.reconnect();

      expect(client.getModesCalls, 2);
      expect(
        h.modes.modes,
        const Modes(shuffle: true, repeat: RepeatSetting.one),
      );
      await h.dispose();
    });
  });

  group('changing', () {
    test(
      'shuffle sends the opposite and shows the answer, not a guess',
      () async {
        final client = FakeApiClient();
        final h = LiveHarness(client: client);
        await h.start();
        client.modesGate = Completer<void>();

        final change = h.modesController.toggleShuffle();
        await pumpEventQueue();

        expect(client.setModesCalls, [(shuffle: true, repeat: null)]);
        expect(h.modes.busy, isTrue);
        expect(
          h.modes.modes?.shuffle,
          isFalse,
          reason: 'nothing before the answer',
        );
        expect(
          h.modes.usable,
          isFalse,
          reason: 'buttons off while it is on its way',
        );

        client.modesGate!.complete();
        await change;
        expect(h.modes.modes?.shuffle, isTrue);
        expect(h.modes.busy, isFalse);
        await h.dispose();
      },
    );

    test('what Apple Music reads back wins over what was asked for', () async {
      final client = _StubbornClient();
      final h = LiveHarness(client: client);
      await h.start();

      await h.modesController.toggleShuffle();

      expect(client.setModesCalls, hasLength(1));
      expect(h.modes.modes?.shuffle, isFalse, reason: 'it did not change');
      await h.dispose();
    });

    test('repeat cycles off → all → one → off', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();

      for (var i = 0; i < 4; i++) {
        await h.modesController.cycleRepeat();
      }

      expect(client.setModesCalls.map((c) => c.repeat), [
        RepeatSetting.all,
        RepeatSetting.one,
        RepeatSetting.off,
        RepeatSetting.all,
      ]);
      expect(client.setModesCalls.every((c) => c.shuffle == null), isTrue);
      await h.dispose();
    });

    test('a second press while one is on its way is ignored', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      client.modesGate = Completer<void>();

      final first = h.modesController.toggleShuffle();
      await pumpEventQueue();
      await h.modesController.cycleRepeat();
      client.modesGate!.complete();
      await first;

      expect(client.setModesCalls, hasLength(1));
      await h.dispose();
    });

    test('nothing is sent before the first answer is known', () async {
      final client = FakeApiClient()..modesError = _notSupported;
      final h = LiveHarness(client: client);
      await h.start();

      await h.modesController.toggleShuffle();
      await h.modesController.cycleRepeat();

      expect(client.setModesCalls, isEmpty);
      await h.dispose();
    });
  });

  group('failures', () {
    test('501 turns off only these buttons', () async {
      final client = FakeApiClient()..modesError = _notSupported;
      final h = LiveHarness(client: client);
      await h.start();

      expect(h.modes.unsupported, isTrue);
      expect(h.modes.usable, isFalse);
      expect(h.modes.notRunning, isFalse, reason: 'not the empty state');
      expect(h.modes.error, isNull, reason: 'no message for a plain 501');
      await h.dispose();
    });

    test('an agent that does not know the route (404) is the same', () async {
      final client = FakeApiClient()..modesError = _noRoute;
      final h = LiveHarness(client: client);
      await h.start();

      expect(h.modes.unsupported, isTrue);
      await h.dispose();
    });

    test('409 marks them stale and says Apple Music is not running', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      expect(h.modes.usable, isTrue);

      client.modesError = _notRunning;
      await h.modesController.load();

      expect(h.modes.notRunning, isTrue);
      expect(h.modes.stale, isTrue);
      expect(h.modes.usable, isFalse);

      // Apple Music is back: the next answer is fresh.
      client.modesError = null;
      await h.modesController.load();
      expect(h.modes.notRunning, isFalse);
      expect(h.modes.usable, isTrue);
      await h.dispose();
    });

    test('another failure while changing shows a message and the buttons come '
        'back', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();

      client.modesError = const ApiException(
        'INTERNAL',
        'state did not change',
        500,
      );
      await h.modesController.toggleShuffle();

      expect(h.modes.error, isNotNull);
      expect(h.modes.errorId, 1);
      expect(h.modes.busy, isFalse);
      expect(h.modes.usable, isTrue);
      await h.dispose();
    });

    test('501 on a change turns the buttons off', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();

      client.modesError = _notSupported;
      await h.modesController.cycleRepeat();

      expect(h.modes.unsupported, isTrue);
      expect(h.modes.usable, isFalse);
      await h.dispose();
    });

    test('a failed read keeps the last known state quietly', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();

      client.modesError = const AgentUnreachableException();
      await h.modesController.load();

      expect(h.modes.modes, const Modes());
      expect(h.modes.error, isNull);
      await h.dispose();
    });
  });

  group('Apple Music coming and going', () {
    test(
      'when it stops the state is stale, and a push makes it fresh',
      () async {
        final h = LiveHarness();
        await h.start();
        expect(h.modes.usable, isTrue);

        await h.push(wsFrame('apple_music', {'running': false}));
        expect(h.modes.stale, isTrue);
        expect(h.modes.usable, isFalse);

        await h.push(wsFrame('modes', {'shuffle': true, 'repeat': 'off'}));
        expect(h.modes.stale, isFalse);
        expect(h.modes.usable, isTrue);
        await h.dispose();
      },
    );

    test('when it starts again the agent is asked again', () async {
      final client = FakeApiClient();
      final h = LiveHarness(client: client);
      await h.start();
      await h.push(wsFrame('apple_music', {'running': false}));
      final reads = client.getModesCalls;

      client.modes = const Modes(shuffle: true, repeat: RepeatSetting.all);
      await h.push(wsFrame('apple_music', {'running': true}));

      expect(client.getModesCalls, reads + 1);
      expect(
        h.modes.modes,
        const Modes(shuffle: true, repeat: RepeatSetting.all),
      );
      expect(h.modes.usable, isTrue);
      await h.dispose();
    });
  });
}
