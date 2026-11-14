import 'dart:async';
import 'dart:typed_data';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/core/models/volume.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/player/live_controller.dart';
import 'package:am_remote/features/player/player_controller.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

class _Harness {
  _Harness({
    FakeApiClient? client,
    InMemorySessionStorage? storage,
    Duration? artworkRetryDelay,

    /// How long the socket waits before reconnecting; short for tests that
    /// drop it. By default the real backoff (1 s) applies.
    Duration? reconnectDelay,
  }) : client = client ?? FakeApiClient() {
    ws = WsClient(
      url: testAgent.webSocketUri(fakeToken),
      connector: connector.call,
      minBackoff: reconnectDelay ?? const Duration(seconds: 1),
      maxBackoff: reconnectDelay ?? const Duration(seconds: 30),
    );
    container = ProviderContainer(
      overrides: [
        ...testOverrides(client: this.client, ws: ws, storage: storage),
        apiClientProvider.overrideWithValue(this.client),
        if (artworkRetryDelay != null)
          artworkRetryDelayProvider.overrideWithValue(artworkRetryDelay),
      ],
    );
    // Keep the controllers alive for the whole test.
    container.listen(liveControllerProvider, (_, _) {});
    container.listen(playerControllerProvider, (_, _) {});
  }

  final FakeApiClient client;
  final connector = FakeWsConnector();
  late final WsClient ws;
  late final ProviderContainer container;

  LiveState get live => container.read(liveControllerProvider);

  PlayerControlsState get controls => container.read(playerControllerProvider);

  /// Connects and lets the first `GET /player/state` finish.
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

Uint8List _bytes(int n) => Uint8List.fromList([n]);

const _noAudioSession = ApiException(
  'NO_AUDIO_SESSION',
  'Apple Music has no audio session yet',
  409,
);
const _notRunning = ApiException(
  'APPLE_MUSIC_NOT_RUNNING',
  'Apple Music is not running',
  409,
);

void main() {
  group('the agent\'s connect order: state, apple_music, then volume', () {
    test('a first connection fills the player, the running flag and the '
        'volume', () async {
      final h = _Harness();
      await h.start();

      await h.push(wsStateFrame(title: 'First Song'));
      await h.push(wsFrame('apple_music', {'running': true}));
      await h.push(wsFrame('volume', {'level': 64, 'muted': true}));

      expect(h.live.player?.title, 'First Song');
      expect(h.live.appleMusicRunning, isTrue);
      expect(h.live.volume?.level, 64);
      expect(h.controls.volume, 64);
      expect(h.controls.muted, isTrue);
      expect(h.controls.volumeLoaded, isTrue);
      await h.dispose();
    });

    test('a reconnect where Apple Music quit: the burst says not running, has '
        'no volume, and the old level is not kept', () async {
      final client = FakeApiClient();
      final h = _Harness(
        client: client,
        reconnectDelay: const Duration(milliseconds: 10),
      );
      await h.start();
      await h.push(wsStateFrame(title: 'First Song'));
      await h.push(wsFrame('apple_music', {'running': true}));
      await h.push(wsFrame('volume', {'level': 64, 'muted': false}));
      expect(h.controls.volumeLoaded, isTrue);

      // Apple Music quit while the socket was down: no media session at all.
      client.volumeError = _notRunning;
      final readsBefore = client.getVolumeCalls;
      await h.reconnect();
      await h.push(wsStateFrame(status: 'stopped', title: '', durationMs: 0));
      await h.push(wsFrame('apple_music', {'running': false}));
      // No `volume` frame follows: there is no audio session.

      expect(h.live.appleMusicRunning, isFalse);
      expect(h.live.player?.status, PlaybackStatus.stopped);
      expect(client.getVolumeCalls, greaterThan(readsBefore));
      expect(h.controls.volumeLoaded, isFalse, reason: '64 is stale now');
      expect(h.controls.notRunning, isTrue);
      await h.dispose();
    });

    test('a reconnect where Apple Music runs but nothing has played: only the '
        'volume goes off', () async {
      final client = FakeApiClient();
      final h = _Harness(
        client: client,
        reconnectDelay: const Duration(milliseconds: 10),
      );
      await h.start();
      await h.push(wsFrame('apple_music', {'running': true}));
      await h.push(wsFrame('volume', {'level': 64, 'muted': false}));

      client.volumeError = _noAudioSession;
      await h.reconnect();
      await h.push(wsStateFrame(status: 'stopped', title: '', durationMs: 0));
      await h.push(wsFrame('apple_music', {'running': true}));

      expect(h.live.appleMusicRunning, isTrue);
      expect(h.controls.volumeUnavailable, isTrue);
      expect(h.controls.volumeLoaded, isFalse);
      expect(h.controls.notRunning, isFalse);
      await h.dispose();
    });

    test('a reconnect that does bring a volume shows it', () async {
      final client = FakeApiClient();
      final h = _Harness(
        client: client,
        reconnectDelay: const Duration(milliseconds: 10),
      );
      await h.start();
      await h.push(wsFrame('volume', {'level': 20, 'muted': false}));

      client.volume = const Volume(level: 80, muted: false);
      await h.reconnect();
      await h.push(wsStateFrame());
      await h.push(wsFrame('apple_music', {'running': true}));
      await h.push(wsFrame('volume', {'level': 80, 'muted': false}));

      expect(h.controls.volume, 80);
      expect(h.controls.volumeLoaded, isTrue);
      await h.dispose();
    });

    test(
      'the first connection is not a reconnect: no extra volume read',
      () async {
        final client = FakeApiClient();
        final h = _Harness(client: client);
        await h.start();
        // (No `apple_music` frame here: a first "running" already has its own
        // reason to read the volume, which is not what this test is about.)
        await h.push(wsStateFrame());
        await h.push(wsFrame('volume', {'level': 42, 'muted': false}));

        expect(client.getVolumeCalls, 1, reason: 'only the first load');
        await h.dispose();
      },
    );
  });

  group('the last volume level can go stale', () {
    test(
      'Apple Music stopping marks it stale; running again reads it',
      () async {
        final client = FakeApiClient();
        final h = _Harness(client: client);
        await h.start();
        await h.push(wsFrame('apple_music', {'running': true}));
        expect(h.controls.volumeLoaded, isTrue);

        await h.push(wsFrame('apple_music', {'running': false}));
        expect(h.controls.volumeLoaded, isFalse);

        final readsBefore = client.getVolumeCalls;
        client.volume = const Volume(level: 55, muted: false);
        await h.push(wsFrame('apple_music', {'running': true}));

        expect(client.getVolumeCalls, readsBefore + 1);
        expect(h.controls.volume, 55);
        expect(h.controls.volumeLoaded, isTrue);
        await h.dispose();
      },
    );

    test(
      'with no audio session, playback starting asks for the volume again',
      () async {
        final client = FakeApiClient()..volumeError = _noAudioSession;
        final h = _Harness(client: client);
        await h.start();
        await h.push(wsStateFrame(status: 'stopped', artworkRev: 1));
        expect(h.controls.volumeUnavailable, isTrue);

        // The song starts: Windows now has an audio session.
        client.volumeError = null;
        final readsBefore = client.getVolumeCalls;
        await h.push(wsStateFrame(status: 'playing', artworkRev: 2));

        expect(client.getVolumeCalls, greaterThan(readsBefore));
        expect(h.controls.volumeUnavailable, isFalse);
        expect(h.controls.volumeLoaded, isTrue);
        await h.dispose();
      },
    );

    test(
      'while the volume is fine, playback changes do not read it again',
      () async {
        final client = FakeApiClient();
        final h = _Harness(client: client);
        await h.start();
        final readsBefore = client.getVolumeCalls;

        await h.push(wsStateFrame(status: 'playing', artworkRev: 2));
        await h.push(wsStateFrame(status: 'playing', artworkRev: 3));

        expect(client.getVolumeCalls, readsBefore);
        await h.dispose();
      },
    );
  });

  test('a pushed state is shown and timestamped', () async {
    final h = _Harness();
    await h.start();
    expect(h.live.link, WsLinkStatus.connected);

    final before = DateTime.now();
    await h.push(wsStateFrame(status: 'paused', positionMs: 5000));

    expect(h.live.player?.status, PlaybackStatus.paused);
    expect(h.live.player?.title, 'Example Song');
    expect(h.live.player?.positionMs, 5000);
    expect(
      h.live.playerReceivedAt!.isBefore(before),
      isFalse,
      reason: 'receivedAt is when the push arrived',
    );
    await h.dispose();
  });

  test(
    'the state is read once over HTTP so the screen need not wait',
    () async {
      final client = FakeApiClient()
        ..playerState = const PlayerState(
          status: PlaybackStatus.playing,
          title: 'From HTTP',
          artworkRev: 3,
        );
      final h = _Harness(client: client);
      await h.start();

      expect(h.live.player?.title, 'From HTTP');
      expect(client.getPlayerStateCalls, 1);
      await h.dispose();
    },
  );

  test('a push that came before the HTTP answer is not overwritten', () async {
    final client = FakeApiClient()
      ..playerState = const PlayerState(title: 'Old HTTP answer');
    final h = _Harness(client: client);
    h.ws.start();
    await pumpEventQueue();
    // (the HTTP answer was already applied: now a push replaces it)
    await h.push(wsStateFrame(title: 'Newer push'));
    expect(h.live.player?.title, 'Newer push');
    await h.dispose();
  });

  test('artwork is fetched again only when artwork_rev changes', () async {
    // The initial HTTP state is at revision 7, so that is fetched once.
    final client = FakeApiClient()
      ..artwork = _bytes(7)
      ..playerState = const PlayerState(artworkRev: 7);
    final h = _Harness(client: client);
    await h.start();
    expect(client.artworkCalls, 1);
    expect(h.live.artwork, _bytes(7));

    // Same revision pushed: no new request, however often the state changes.
    await h.push(wsStateFrame(artworkRev: 7));
    await h.push(wsStateFrame(artworkRev: 7, positionMs: 50000));
    await h.push(wsStateFrame(artworkRev: 7, positionMs: 60000));
    expect(client.artworkCalls, 1);

    client.artwork = _bytes(8);
    await h.push(wsStateFrame(artworkRev: 8));
    expect(client.artworkCalls, 2);
    expect(h.live.artwork, _bytes(8));

    await h.dispose();
  });

  test('a late answer for an old artwork revision is dropped', () async {
    final client = FakeApiClient()..artworkGate = Completer<void>();
    final h = _Harness(client: client);
    await h.start();

    // Request 1 is the initial state's revision 0; it waits on the gate too.
    expect(client.artworkCalls, 1);
    client.artwork = _bytes(8);
    await h.push(wsStateFrame(artworkRev: 8)); // request 2
    client.artwork = _bytes(9);
    await h.push(wsStateFrame(artworkRev: 9)); // request 3
    expect(client.artworkCalls, 3);

    client.artworkGate!.complete();
    await pumpEventQueue();

    expect(h.live.artwork, _bytes(9));
    await h.dispose();
  });

  test(
    'a failed artwork request is not "no artwork" and is tried again',
    () async {
      final client = FakeApiClient()..artwork = _bytes(7);
      final h = _Harness(client: client);
      await h.start();

      // The first revision loads fine.
      await h.push(wsStateFrame(artworkRev: 1));
      expect(h.live.artwork, _bytes(7));

      // Revision 2: the request fails (Wi-Fi blip). The old picture stays, and
      // the placeholder flag is not set.
      client.artworkError = const AgentUnreachableException();
      await h.push(wsStateFrame(artworkRev: 2));
      expect(h.live.artwork, _bytes(7));
      expect(h.live.artworkMissing, isFalse);
      final failedCalls = client.artworkCalls;

      // Pushes right after the failure do not hammer the endpoint.
      await h.push(wsStateFrame(artworkRev: 2, positionMs: 50000));
      expect(client.artworkCalls, failedCalls);

      await h.dispose();
    },
  );

  test(
    'artwork is requested again after the pause once a request failed',
    () async {
      final client = FakeApiClient()..artwork = _bytes(7);
      client.artworkError = const AgentUnreachableException();
      // No wait between tries, standing in for the time that passes.
      final h = _Harness(client: client, artworkRetryDelay: Duration.zero);
      await h.start();
      await h.push(wsStateFrame(artworkRev: 3)); // fails
      final failedCalls = client.artworkCalls;

      client.artworkError = null;
      await h.push(wsStateFrame(artworkRev: 3, positionMs: 60000));

      expect(client.artworkCalls, failedCalls + 1);
      expect(h.live.artwork, _bytes(7));
      await h.dispose();
    },
  );

  test('when the socket drops the connection is checked at once', () async {
    final client = FakeApiClient();
    final h = _Harness(
      client: client,
      storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
    );
    await h.start();
    final before = client.statusCalls;

    h.connector.last.serverDrop();
    await pumpEventQueue();

    expect(client.statusCalls, greaterThan(before));
    await h.dispose();
  });

  test('a 401 found by that check sends the app back to pairing', () async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    final client = FakeApiClient();
    final h = _Harness(client: client, storage: storage);
    await h.start();
    expect(h.container.read(sessionControllerProvider), isA<SessionPaired>());

    // The token was revoked: the socket is refused, and so is /status.
    client.statusError = const ApiException('UNAUTHORIZED', 'revoked', 401);
    h.connector.last.serverDrop();
    await pumpEventQueue();

    expect(h.container.read(sessionControllerProvider), isA<SessionUnpaired>());
    expect(storage.token, isNull);
    await h.dispose();
  });

  test('no artwork (404) shows the placeholder state', () async {
    final client = FakeApiClient()..artwork = null;
    final h = _Harness(client: client);
    await h.start();

    await h.push(wsStateFrame(artworkRev: 1));

    expect(h.live.artwork, isNull);
    expect(h.live.artworkMissing, isTrue);
    await h.dispose();
  });

  test('apple_music events set whether Apple Music is running', () async {
    final h = _Harness();
    await h.start();
    expect(h.live.appleMusicRunning, isNull);

    await h.push(wsFrame('apple_music', {'running': false}));
    expect(h.live.appleMusicRunning, isFalse);

    await h.push(wsFrame('apple_music', {'running': true}));
    expect(h.live.appleMusicRunning, isTrue);
    await h.dispose();
  });

  test('a pushed volume moves the slider', () async {
    final h = _Harness();
    await h.start();
    expect(h.controls.volume, 42);

    await h.push(wsFrame('volume', {'level': 65, 'muted': true}));

    expect(h.live.volume, const Volume(level: 65, muted: true));
    expect((h.controls.volume, h.controls.muted), (65, true));
    await h.dispose();
  });

  test(
    'a pushed volume does not move the slider while it is dragged',
    () async {
      final h = _Harness();
      await h.start();
      final controller = h.container.read(playerControllerProvider.notifier);

      controller.volumeDragStart();
      controller.volumeChanged(20);
      await h.push(wsFrame('volume', {'level': 90, 'muted': false}));
      expect(h.controls.volume, 20, reason: 'the finger wins while dragging');

      controller.volumeDragEnd(20);
      await pumpEventQueue();
      await h.dispose();
    },
  );

  test('apple_music running:true clears the empty state and reloads', () async {
    final client = FakeApiClient();
    final h = _Harness(client: client);
    await h.start();
    client.commandError = const ApiException(
      'APPLE_MUSIC_NOT_RUNNING',
      'closed',
      409,
    );
    await h.container.read(playerControllerProvider.notifier).toggle();
    expect(h.controls.notRunning, isTrue);

    client.commandError = null;
    final loads = client.getVolumeCalls;
    await h.push(wsFrame('apple_music', {'running': false}));
    await h.push(wsFrame('apple_music', {'running': true}));

    expect(h.controls.notRunning, isFalse);
    expect(client.getVolumeCalls, greaterThan(loads));
    await h.dispose();
  });

  test('seek sends the position to the agent', () async {
    final client = FakeApiClient();
    final h = _Harness(client: client);
    await h.start();

    await h.container.read(playerControllerProvider.notifier).seek(90000);
    await h.container.read(playerControllerProvider.notifier).seek(-5);

    expect(client.seekCalls, [90000, 0]);
    await h.dispose();
  });

  test(
    'seek never goes past the end of the track (the agent says 400)',
    () async {
      final client = FakeApiClient();
      final h = _Harness(client: client);
      await h.start();
      await h.push(wsStateFrame(durationMs: 180000));

      await h.container.read(playerControllerProvider.notifier).seek(999999);

      expect(client.seekCalls, [180000]);
      await h.dispose();
    },
  );
}
