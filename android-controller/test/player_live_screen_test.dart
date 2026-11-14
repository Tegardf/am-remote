import 'dart:convert';
import 'dart:typed_data';

import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:am_remote/features/player/progress.dart';
import 'package:am_remote/features/player/progress_bar.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

/// A valid 1x1 PNG, so `Image.memory` has something it can decode.
final _png = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

class _Live {
  _Live(this.client);

  final FakeApiClient client;
  final connector = FakeWsConnector();
  late final WsClient ws;

  /// Shows the app paired with the fake agent and connects the fake socket.
  Future<void> pump(WidgetTester tester) async {
    ws = WsClient(
      url: testAgent.webSocketUri(fakeToken),
      connector: connector.call,
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: testOverrides(
          client: client,
          storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
          ws: ws,
        ),
        child: const MaterialApp(home: AppRouter()),
      ),
    );
    ws.start();
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(const Duration(milliseconds: 50));
  }

  /// The agent pushes a frame; the bar of a playing track keeps ticking, so
  /// pump a fixed time instead of waiting to settle.
  Future<void> push(WidgetTester tester, String frame) async {
    connector.last.serverSend(frame);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(const Duration(milliseconds: 50));
  }

  // Not awaited: its timers stop at once, and awaiting a close inside a widget
  // test would wait on a clock the test controls.
  void dispose() => ws.dispose();
}

void main() {
  testWidgets('the play/pause icon follows the status the laptop pushes', (
    tester,
  ) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);
    expect(find.byIcon(Icons.play_arrow), findsOneWidget, reason: 'idle');

    await live.push(tester, wsStateFrame(status: 'playing'));
    expect(find.byIcon(Icons.pause), findsOneWidget);
    expect(find.byIcon(Icons.play_arrow), findsNothing);

    await live.push(tester, wsStateFrame(status: 'paused'));
    expect(find.byIcon(Icons.play_arrow), findsOneWidget);
    expect(find.byIcon(Icons.pause), findsNothing);

    await live.push(tester, wsStateFrame(status: 'playing'));
    await live.push(tester, wsStateFrame(status: 'stopped'));
    expect(find.byIcon(Icons.play_arrow), findsOneWidget);
    live.dispose();
  });

  testWidgets(
    'tapping play/pause sends toggle and leaves the icon to the push',
    (tester) async {
      final client = FakeApiClient();
      final live = _Live(client);
      await live.pump(tester);
      await live.push(tester, wsStateFrame(status: 'paused'));

      await tester.tap(find.byTooltip('Play / pause'));
      await tester.pump(const Duration(milliseconds: 50));
      expect(client.calls, ['toggle']);
      expect(
        find.byIcon(Icons.play_arrow),
        findsOneWidget,
        reason: 'no optimistic change before the push',
      );

      await live.push(tester, wsStateFrame(status: 'playing'));
      expect(find.byIcon(Icons.pause), findsOneWidget);
      live.dispose();
    },
  );

  testWidgets('shows title, artist, album and the position', (tester) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);

    await live.push(
      tester,
      wsStateFrame(
        status: 'paused',
        title: 'Blue Monday',
        artist: 'New Order',
        album: 'Power, Corruption & Lies',
        positionMs: 42000,
        durationMs: 215000,
      ),
    );

    expect(find.text('Blue Monday'), findsOneWidget);
    expect(find.text('New Order · Power, Corruption & Lies'), findsOneWidget);
    expect(find.text('0:42'), findsOneWidget);
    expect(find.text('3:35'), findsOneWidget);
    live.dispose();
  });

  testWidgets('shows the playlist name when the agent sends one', (
    tester,
  ) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);

    await live.push(tester, wsStateFrame(playlistName: 'Road Trip'));

    expect(find.text('Playlist: Road Trip'), findsOneWidget);
    live.dispose();
  });

  testWidgets('shows "Nothing playing" for an empty title', (tester) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);

    await live.push(tester, wsStateFrame(status: 'stopped', title: ''));

    expect(find.text('Nothing playing'), findsOneWidget);
    live.dispose();
  });

  testWidgets('shows the artwork, and a placeholder when there is none', (
    tester,
  ) async {
    final client = FakeApiClient()..artwork = _png;
    final live = _Live(client);
    await live.pump(tester);

    await live.push(tester, wsStateFrame(artworkRev: 1));
    expect(find.byKey(const ValueKey('artwork')), findsOneWidget);
    expect(find.byKey(const ValueKey('artwork-placeholder')), findsNothing);

    client.artwork = null; // the next track has no artwork: 404
    await live.push(tester, wsStateFrame(artworkRev: 2));
    expect(find.byKey(const ValueKey('artwork')), findsNothing);
    expect(find.byKey(const ValueKey('artwork-placeholder')), findsOneWidget);
    live.dispose();
  });

  testWidgets(
    'apple_music running:false shows the empty state, true brings back the controls',
    (tester) async {
      final live = _Live(FakeApiClient());
      await live.pump(tester);
      await live.push(tester, wsStateFrame());
      expect(find.byTooltip('Play / pause'), findsOneWidget);

      await live.push(tester, wsFrame('apple_music', {'running': false}));
      expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
      expect(find.byTooltip('Play / pause'), findsNothing);

      await live.push(tester, wsFrame('apple_music', {'running': true}));
      expect(find.text('Open Apple Music on your laptop'), findsNothing);
      expect(find.byTooltip('Play / pause'), findsOneWidget);
      live.dispose();
    },
  );

  testWidgets('a volume push moves the volume number', (tester) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);
    expect(find.text('42'), findsWidgets);

    await live.push(tester, wsFrame('volume', {'level': 77, 'muted': false}));

    expect(find.text('77'), findsWidgets);
    live.dispose();
  });

  testWidgets('dragging the progress bar seeks when the finger lifts', (
    tester,
  ) async {
    final client = FakeApiClient();
    final live = _Live(client);
    await live.pump(tester);
    await live.push(tester, wsStateFrame(status: 'paused', positionMs: 42000));

    final slider = find.descendant(
      of: find.byType(ProgressBar),
      matching: find.byType(Slider),
    );
    await tester.drag(slider, const Offset(60, 0));
    await tester.pump(const Duration(milliseconds: 50));

    expect(client.seekCalls, hasLength(1));
    expect(client.seekCalls.single, greaterThan(42000));
    expect(client.seekCalls.single, lessThanOrEqualTo(215000));

    // The agent answers with the new position: the bar follows the push.
    final target = client.seekCalls.single;
    await live.push(
      tester,
      wsStateFrame(status: 'paused', positionMs: target + 500),
    );
    expect(find.text(formatPosition(target + 500)), findsOneWidget);
    live.dispose();
  });

  testWidgets('a push that was already on its way does not snap the bar back', (
    tester,
  ) async {
    final client = FakeApiClient();
    final live = _Live(client);
    await live.pump(tester);
    await live.push(tester, wsStateFrame(status: 'paused', positionMs: 42000));

    final slider = find.descendant(
      of: find.byType(ProgressBar),
      matching: find.byType(Slider),
    );
    await tester.drag(slider, const Offset(60, 0));
    await tester.pump(const Duration(milliseconds: 50));
    final target = client.seekCalls.single;

    // A state sent before the agent applied the seek: still the old position.
    await live.push(tester, wsStateFrame(status: 'paused', positionMs: 42000));
    expect(find.text(formatPosition(target)), findsOneWidget);
    expect(find.text('0:42'), findsNothing);

    // The push that carries the new position confirms it.
    await live.push(
      tester,
      wsStateFrame(status: 'paused', positionMs: target + 200),
    );
    expect(find.text(formatPosition(target + 200)), findsOneWidget);
    live.dispose();
  });

  testWidgets('without a confirming push the bar goes back after 2 s', (
    tester,
  ) async {
    final client = FakeApiClient();
    final live = _Live(client);
    await live.pump(tester);
    await live.push(tester, wsStateFrame(status: 'paused', positionMs: 42000));

    final slider = find.descendant(
      of: find.byType(ProgressBar),
      matching: find.byType(Slider),
    );
    await tester.drag(slider, const Offset(60, 0));
    await tester.pump(const Duration(milliseconds: 50));
    final target = client.seekCalls.single;
    expect(find.text(formatPosition(target)), findsOneWidget);

    await tester.pump(const Duration(seconds: 3));

    // The agent never moved: the bar shows what it said last.
    expect(find.text('0:42'), findsOneWidget);
    live.dispose();
  });

  testWidgets('a track that is not playing does not move on its own', (
    tester,
  ) async {
    final live = _Live(FakeApiClient());
    await live.pump(tester);
    await live.push(tester, wsStateFrame(status: 'paused', positionMs: 42000));

    await tester.pump(const Duration(seconds: 3));

    expect(find.text('0:42'), findsOneWidget);
    live.dispose();
  });

  testWidgets('an unreachable laptop shows the offline banner', (tester) async {
    final client = FakeApiClient()
      ..statusError = const AgentUnreachableException();
    final live = _Live(client);
    await live.pump(tester);

    expect(find.textContaining('No connection to your laptop'), findsOneWidget);
    expect(find.text('Offline'), findsOneWidget);
    live.dispose();
  });

  test('the test PNG is a real PNG', () {
    expect(Uint8List.fromList(_png.take(4).toList()), [0x89, 0x50, 0x4E, 0x47]);
  });
}
