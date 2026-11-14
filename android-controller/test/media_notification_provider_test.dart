import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/player/media_notification.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

const _playing = PlayerState(
  status: PlaybackStatus.playing,
  title: 'Example Song',
  artist: 'Example Artist',
  album: 'Example Album',
  durationMs: 215000,
  positionMs: 42000,
);

class _RecordingCommands implements MediaCommands {
  final calls = <String>[];

  @override
  Future<void> play() async => calls.add('play');

  @override
  Future<void> pause() async => calls.add('pause');

  @override
  Future<void> next() async => calls.add('next');

  @override
  Future<void> previous() async => calls.add('previous');

  @override
  Future<void> seek(int positionMs) async => calls.add('seek $positionMs');
}

class _Harness {
  _Harness({FakeApiClient? client, InMemorySessionStorage? storage})
    : client = client ?? FakeApiClient() {
    ws = WsClient(
      url: testAgent.webSocketUri(fakeToken),
      connector: connector.call,
    );
    container = ProviderContainer(
      overrides: [
        ...testOverrides(
          client: this.client,
          ws: ws,
          storage:
              storage ??
              InMemorySessionStorage(token: fakeToken, agent: testAgent),
        ),
        apiClientProvider.overrideWithValue(this.client),
        audioHandlerProvider.overrideWithValue(handler),
      ],
    );
    // What AppRouter does: keep the bridge alive.
    container.listen(mediaNotificationProvider, (_, _) {});
  }

  final FakeApiClient client;
  final connector = FakeWsConnector();
  final handler = AmRemoteAudioHandler();
  late final WsClient ws;
  late final ProviderContainer container;

  Future<void> start() async {
    await container.read(sessionControllerProvider.notifier).ready;
    ws.start();
    await pumpEventQueue();
  }

  Future<void> push(String frame) async {
    connector.last.serverSend(frame);
    await pumpEventQueue();
  }

  Future<void> dispose() async {
    container.dispose();
    await ws.dispose();
  }
}

void main() {
  group('AmRemoteAudioHandler', () {
    test('showPlayer mirrors the track and the playing state', () {
      final handler = AmRemoteAudioHandler();
      handler.showPlayer(_playing, now: DateTime(2026, 11, 14));

      expect(handler.mediaItem.value?.title, 'Example Song');
      expect(handler.playbackState.value.playing, isTrue);
    });

    test('clear removes the track and goes idle', () {
      final handler = AmRemoteAudioHandler();
      handler.showPlayer(_playing);

      handler.clear();

      expect(handler.mediaItem.value, isNull);
      expect(handler.playbackState.value.playing, isFalse);
    });

    test(
      'the notification buttons are forwarded to the agent commands',
      () async {
        final handler = AmRemoteAudioHandler();
        final commands = _RecordingCommands();
        handler.commands = commands;

        await handler.play();
        await handler.pause();
        await handler.skipToNext();
        await handler.skipToPrevious();
        await handler.seek(const Duration(seconds: 90));

        expect(commands.calls, [
          'play',
          'pause',
          'next',
          'previous',
          'seek 90000',
        ]);
      },
    );

    test(
      'buttons do nothing without commands, and errors do not escape',
      () async {
        final handler = AmRemoteAudioHandler();
        await handler.play(); // no commands: nothing happens, nothing thrown

        handler.commands = _ThrowingCommands();
        await handler.skipToNext(); // the failure is swallowed
      },
    );

    test('a swiped-away app takes the notification down', () async {
      final handler = AmRemoteAudioHandler();
      handler.showPlayer(_playing);

      await handler.onTaskRemoved();

      expect(handler.mediaItem.value, isNull);
      expect(handler.playbackState.value.playing, isFalse);
    });
  });

  group('mediaNotificationProvider', () {
    test('shows the pushed track and wires the buttons to the API', () async {
      final h = _Harness();
      await h.start();

      await h.push(wsStateFrame(title: 'Blue Monday', status: 'playing'));
      expect(h.handler.mediaItem.value?.title, 'Blue Monday');
      expect(h.handler.playbackState.value.playing, isTrue);

      await h.handler.pause();
      await h.handler.skipToNext();
      expect(h.client.calls, ['pause', 'next']);
      await h.dispose();
    });

    test('Apple Music closing clears it, and opening shows it again', () async {
      final h = _Harness();
      await h.start();
      await h.push(wsStateFrame(title: 'Blue Monday'));

      await h.push(wsFrame('apple_music', {'running': false}));
      expect(h.handler.mediaItem.value, isNull);

      await h.push(wsFrame('apple_music', {'running': true}));
      expect(h.handler.mediaItem.value?.title, 'Blue Monday');
      await h.dispose();
    });

    test(
      'losing the connection clears it, getting it back shows it again',
      () async {
        final h = _Harness();
        await h.start();
        await h.push(wsStateFrame(title: 'Blue Monday'));
        expect(h.handler.mediaItem.value?.title, 'Blue Monday');

        h.client.statusError = const AgentUnreachableException();
        await h.container
            .read(sessionControllerProvider.notifier)
            .checkConnection();
        await pumpEventQueue();
        expect(h.handler.mediaItem.value, isNull);

        // Back online, with the same track as before.
        h.client.statusError = null;
        await h.container
            .read(sessionControllerProvider.notifier)
            .checkConnection();
        await pumpEventQueue();
        expect(h.handler.mediaItem.value?.title, 'Blue Monday');
        await h.dispose();
      },
    );

    test('unpairing clears it and detaches the buttons', () async {
      final h = _Harness();
      await h.start();
      await h.push(wsStateFrame(title: 'Blue Monday'));
      expect(h.handler.commands, isNotNull);

      await h.container
          .read(sessionControllerProvider.notifier)
          .handleUnauthorized();
      await pumpEventQueue();

      expect(h.handler.mediaItem.value, isNull);
      expect(h.handler.commands, isNull);
      await h.dispose();
    });
  });
}

class _ThrowingCommands implements MediaCommands {
  @override
  Future<void> play() => throw const AgentUnreachableException();

  @override
  Future<void> pause() => throw const AgentUnreachableException();

  @override
  Future<void> next() => throw const AgentUnreachableException();

  @override
  Future<void> previous() => throw const AgentUnreachableException();

  @override
  Future<void> seek(int positionMs) => throw const AgentUnreachableException();
}
