import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/volume.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/player/player_controller.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

const _notRunning = ApiException(
  'APPLE_MUSIC_NOT_RUNNING',
  'Apple Music is not running',
  409,
);

const _noAudioSession = ApiException(
  'NO_AUDIO_SESSION',
  'Apple Music has no audio session yet',
  409,
);

ProviderContainer _container(FakeApiClient client) {
  final container = ProviderContainer(
    overrides: [
      ...testOverrides(client: client),
      apiClientProvider.overrideWithValue(client),
    ],
  );
  addTearDown(container.dispose);
  // The controller is autoDispose: keep it alive for the whole test.
  container.listen(playerControllerProvider, (_, _) {});
  return container;
}

void main() {
  group('NO_AUDIO_SESSION (Apple Music is there, but nothing has played)', () {
    test('on GET /volume switches off only the volume', () async {
      final client = FakeApiClient()..volumeError = _noAudioSession;
      final container = _container(client);
      await pumpEventQueue();

      final state = container.read(playerControllerProvider);
      expect(state.volumeUnavailable, isTrue);
      expect(state.volumeLoaded, isFalse, reason: 'slider and mute are off');
      expect(state.notRunning, isFalse, reason: 'not the empty state');
      expect(state.error, isNull, reason: 'no snackbar either');
    });

    test('on POST /volume does the same', () async {
      final client = FakeApiClient();
      final container = _container(client);
      await pumpEventQueue();
      expect(container.read(playerControllerProvider).volumeLoaded, isTrue);

      client.volumeError = _noAudioSession;
      container.read(playerControllerProvider.notifier).volumeDragEnd(30);
      await pumpEventQueue();

      final state = container.read(playerControllerProvider);
      expect(state.volumeUnavailable, isTrue);
      expect(state.volumeLoaded, isFalse);
      expect(state.notRunning, isFalse);
      expect(state.error, isNull);
    });

    test('the transport buttons keep working', () async {
      final client = FakeApiClient()..volumeError = _noAudioSession;
      final container = _container(client);
      await pumpEventQueue();

      await container.read(playerControllerProvider.notifier).toggle();

      expect(client.calls, ['toggle']);
      final state = container.read(playerControllerProvider);
      expect(state.busy, isFalse);
      expect(state.notRunning, isFalse);
    });

    test('a later successful read brings the volume back', () async {
      final client = FakeApiClient()..volumeError = _noAudioSession;
      final container = _container(client);
      await pumpEventQueue();
      expect(container.read(playerControllerProvider).volumeUnavailable, true);

      client.volumeError = null;
      await container.read(playerControllerProvider.notifier).loadVolume();

      final state = container.read(playerControllerProvider);
      expect(state.volumeUnavailable, isFalse);
      expect(state.volumeLoaded, isTrue);
      expect(state.volume, 42);
    });

    test('is not APPLE_MUSIC_NOT_RUNNING: that one still shows the empty '
        'state (no media session at all)', () async {
      final client = FakeApiClient()..volumeError = _notRunning;
      final container = _container(client);
      await pumpEventQueue();

      final state = container.read(playerControllerProvider);
      expect(state.notRunning, isTrue);
      expect(state.volumeUnavailable, isFalse);
    });

    test('APPLE_MUSIC_NOT_RUNNING on a volume call marks a loaded level '
        'stale', () async {
      final client = FakeApiClient();
      final container = _container(client);
      await pumpEventQueue();
      // The level is loaded first, so "stale" means something.
      var state = container.read(playerControllerProvider);
      expect((state.volume, state.volumeLoaded), (42, true));

      client.volumeError = _notRunning; // Apple Music quit
      await container.read(playerControllerProvider.notifier).loadVolume();

      state = container.read(playerControllerProvider);
      expect(state.notRunning, isTrue);
      expect(state.volumeUnavailable, isFalse);
      expect(state.volumeLoaded, isFalse, reason: '42 is stale now');
    });

    test('a 409 not-running after a loaded volume marks the level stale, and '
        'the next success reads it again', () async {
      final client = FakeApiClient();
      final container = _container(client);
      await pumpEventQueue();
      expect(container.read(playerControllerProvider).volumeLoaded, isTrue);

      // Apple Music quit: a command is answered with the 409.
      client.commandError = _notRunning;
      await container.read(playerControllerProvider.notifier).toggle();
      var state = container.read(playerControllerProvider);
      expect(state.notRunning, isTrue);
      expect(state.volumeLoaded, isFalse, reason: '42 is stale');

      // Apple Music is back, at another level: the level is read again.
      client.commandError = null;
      client.volume = const Volume(level: 70, muted: false);
      await container.read(playerControllerProvider.notifier).toggle();
      await pumpEventQueue();

      state = container.read(playerControllerProvider);
      expect(state.notRunning, isFalse);
      expect(state.volumeLoaded, isTrue);
      expect(state.volume, 70);
    });
  });

  test('loads the initial volume from GET /volume', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();

      final state = container.read(playerControllerProvider);
      expect(
        (state.volume, state.muted, state.volumeLoaded),
        (42, false, true),
      );
      expect(client.getVolumeCalls, 1);
    });
  });

  test('10 quick slider changes send one POST with the last value', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      controller.volumeDragStart();
      for (var i = 1; i <= 10; i++) {
        controller.volumeChanged(i * 5);
        async.elapse(const Duration(milliseconds: 5));
      }
      // The slider follows the finger at once, before anything is sent.
      expect(container.read(playerControllerProvider).volume, 50);
      expect(client.setVolumeCalls, isEmpty);

      async.elapse(const Duration(milliseconds: 200));
      async.flushMicrotasks();
      expect(client.setVolumeCalls.map((c) => c.level), [50]);
    });
  });

  test('a long drag is throttled and the release sends the final value', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      controller.volumeDragStart();
      // A 1 second drag: a change every 20 ms, 50 changes.
      for (var i = 1; i <= 50; i++) {
        controller.volumeChanged(i);
        async.elapse(const Duration(milliseconds: 20));
        async.flushMicrotasks();
      }
      expect(client.setVolumeCalls.length, inInclusiveRange(5, 12));

      controller.volumeDragEnd(77);
      async.flushMicrotasks();

      expect(client.setVolumeCalls.last.level, 77);
      final state = container.read(playerControllerProvider);
      expect((state.volume, state.dragging), (77, false));
    });
  });

  test('a value arriving while a request is in flight is sent afterwards', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      controller.volumeDragStart();
      controller.volumeChanged(10);
      async.elapse(const Duration(milliseconds: 100));
      controller.volumeChanged(20);
      async.elapse(const Duration(milliseconds: 100));
      controller.volumeDragEnd(30);
      async.flushMicrotasks();

      expect(client.setVolumeCalls.last.level, 30);
      expect(container.read(playerControllerProvider).volume, 30);
    });
  });

  test('mute sends only "muted" and takes the answer from the agent', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();

      container.read(playerControllerProvider.notifier).toggleMute();
      async.flushMicrotasks();

      expect(client.setVolumeCalls.single, (level: null, muted: true));
      expect(container.read(playerControllerProvider).muted, isTrue);
    });
  });

  test('a second tap while a command is in flight is ignored', () {
    fakeAsync((async) {
      final client = FakeApiClient()..gate = Completer<void>();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      controller.toggle();
      controller.toggle();
      controller.next();
      async.flushMicrotasks();
      expect(client.calls, ['toggle']);
      expect(container.read(playerControllerProvider).busy, isTrue);

      client.gate!.complete();
      async.flushMicrotasks();
      expect(container.read(playerControllerProvider).busy, isFalse);
    });
  });

  test('APPLE_MUSIC_NOT_RUNNING flags the empty state; success clears it', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      client.commandError = _notRunning;
      controller.toggle();
      async.flushMicrotasks();
      expect(container.read(playerControllerProvider).notRunning, isTrue);
      expect(container.read(playerControllerProvider).error, isNull);

      client.commandError = null;
      controller.toggle();
      async.flushMicrotasks();
      expect(container.read(playerControllerProvider).notRunning, isFalse);
    });
  });

  test('an unreachable laptop gives a short error message', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();

      client.commandError = const AgentUnreachableException();
      container.read(playerControllerProvider.notifier).next();
      async.flushMicrotasks();

      final state = container.read(playerControllerProvider);
      expect(state.error, "Can't reach the laptop");
      expect(state.busy, isFalse);
    });
  });

  test('NOT_SUPPORTED explains itself instead of a generic error', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();

      client.commandError = const ApiException(
        'NOT_SUPPORTED',
        'Not on this machine',
        501,
      );
      container.read(playerControllerProvider.notifier).next();
      async.flushMicrotasks();

      expect(
        container.read(playerControllerProvider).error,
        'Not supported on this laptop',
      );
    });
  });

  test(
    'a volume error does not re-enable buttons whose request is running',
    () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final container = _container(client);
        async.flushMicrotasks();
        final controller = container.read(playerControllerProvider.notifier);

        client.gate = Completer<void>();
        controller.toggle();
        async.flushMicrotasks();
        expect(container.read(playerControllerProvider).busy, isTrue);

        client.volumeError = const AgentUnreachableException();
        controller.volumeDragEnd(10);
        async.flushMicrotasks();
        expect(client.setVolumeCalls.map((c) => c.level), [10]);
        expect(container.read(playerControllerProvider).busy, isTrue);

        client.gate!.complete();
        async.flushMicrotasks();
        expect(container.read(playerControllerProvider).busy, isFalse);
      });
    },
  );

  test('a stale GET /volume does not overwrite a newer slider value', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final container = _container(client);
      async.flushMicrotasks();
      final controller = container.read(playerControllerProvider.notifier);

      // The GET starts while the laptop is at 42 and answers late.
      client.getVolumeGate = Completer<void>();
      controller.loadVolume();
      async.flushMicrotasks();

      controller.volumeDragEnd(80);
      async.flushMicrotasks();
      expect(container.read(playerControllerProvider).volume, 80);

      client.getVolumeGate!.complete();
      async.flushMicrotasks();
      expect(container.read(playerControllerProvider).volume, 80);
    });
  });

  test(
    'releasing the slider during a request sends the final value after it',
    () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final container = _container(client);
        async.flushMicrotasks();
        final controller = container.read(playerControllerProvider.notifier);

        client.setVolumeGate = Completer<void>();
        controller.volumeDragStart();
        controller.volumeChanged(10);
        async.elapse(const Duration(milliseconds: 100));
        async.flushMicrotasks();
        expect(client.setVolumeCalls.map((c) => c.level), [10]);

        // Released at 30 while the first request is still running.
        controller.volumeDragEnd(30);
        async.flushMicrotasks();
        expect(client.setVolumeCalls.map((c) => c.level), [10]);
        expect(container.read(playerControllerProvider).volume, 30);

        client.setVolumeGate!.complete();
        async.flushMicrotasks();
        expect(client.setVolumeCalls.map((c) => c.level), [10, 30]);
        // The late answer for 10 never pulled the slider back.
        expect(container.read(playerControllerProvider).volume, 30);
      });
    },
  );

  test(
    'a failed request with a newer value pending still sends that value',
    () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final container = _container(client);
        async.flushMicrotasks();
        final controller = container.read(playerControllerProvider.notifier);

        client.setVolumeGate = Completer<void>();
        controller.volumeDragStart();
        controller.volumeChanged(10);
        async.elapse(const Duration(milliseconds: 100));
        controller.volumeDragEnd(30);
        client.volumeError = const AgentUnreachableException();
        client.setVolumeGate!.complete();
        async.flushMicrotasks();

        expect(client.setVolumeCalls.map((c) => c.level), [10, 30]);
        final state = container.read(playerControllerProvider);
        expect(state.error, "Can't reach the laptop");
        expect(state.busy, isFalse);
      });
    },
  );
}
