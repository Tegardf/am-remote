import 'dart:async';

import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/modes.dart';
import 'package:am_remote/core/models/queue_item.dart';
import 'package:am_remote/core/ws_client.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

const _notSupported = ApiException('NOT_SUPPORTED', 'no buttons', 501);
const _notRunning = ApiException('APPLE_MUSIC_NOT_RUNNING', 'closed', 409);

Future<void> _pump(WidgetTester tester, FakeApiClient client) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: testOverrides(
        client: client,
        storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
      ),
      child: const MaterialApp(home: AppRouter()),
    ),
  );
  await tester.pumpAndSettle();
}

/// The app with a fake socket, so the laptop can push frames.
class _Live {
  _Live(this.client);

  final FakeApiClient client;
  final connector = FakeWsConnector();
  late final WsClient ws;

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

  Future<void> push(WidgetTester tester, String frame) async {
    connector.last.serverSend(frame);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pump(const Duration(milliseconds: 50));
  }

  void dispose() => ws.dispose();
}

/// A phone-sized screen (411 x 890): the default test screen is only 600 high,
/// which hides the lower part of the Player screen.
void _test(String name, Future<void> Function(WidgetTester tester) body) {
  testWidgets(name, (tester) async {
    tester.view.devicePixelRatio = 1.0;
    tester.view.physicalSize = const Size(411, 890);
    addTearDown(tester.view.reset);
    await body(tester);
  });
}

/// Scrolls [finder] into view and taps it, as a user would.
Future<void> _tapVisible(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.pumpAndSettle();
  await tester.tap(finder);
}

IconButton _button(WidgetTester tester, String tooltip) =>
    tester.widget<IconButton>(
      find.ancestor(
        of: find.byTooltip(tooltip),
        matching: find.byType(IconButton),
      ),
    );

void main() {
  group('shuffle and repeat', () {
    _test('are next to the transport buttons, both off at first', (
      tester,
    ) async {
      await _pump(tester, FakeApiClient());

      expect(find.byTooltip('Shuffle off'), findsOneWidget);
      expect(find.byTooltip('Repeat off'), findsOneWidget);
      expect(find.byTooltip('Play / pause'), findsOneWidget);
      expect(_button(tester, 'Shuffle off').onPressed, isNotNull);
    });

    _test('shuffle sends the command and shows the answer', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);

      await tester.tap(find.byTooltip('Shuffle off'));
      await tester.pumpAndSettle();

      expect(client.setModesCalls, [(shuffle: true, repeat: null)]);
      expect(find.byTooltip('Shuffle on'), findsOneWidget);
      expect(_button(tester, 'Shuffle on').isSelected, isTrue);
    });

    _test('nothing changes before the agent answers, and the buttons are '
        'off meanwhile', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);
      client.modesGate = Completer<void>();

      await tester.tap(find.byTooltip('Shuffle off'));
      await tester.pump();

      expect(find.byTooltip('Shuffle off'), findsOneWidget, reason: 'no guess');
      expect(_button(tester, 'Shuffle off').onPressed, isNull);
      expect(_button(tester, 'Repeat off').onPressed, isNull);

      client.modesGate!.complete();
      await tester.pumpAndSettle();
      expect(find.byTooltip('Shuffle on'), findsOneWidget);
    });

    _test('repeat cycles off → all → one → off, with its own icon each '
        'time', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);
      expect(find.byIcon(Icons.repeat), findsOneWidget);

      await tester.tap(find.byTooltip('Repeat off'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Repeat all'), findsOneWidget);
      expect(find.byIcon(Icons.repeat), findsOneWidget);
      expect(_button(tester, 'Repeat all').isSelected, isTrue);

      await tester.tap(find.byTooltip('Repeat all'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Repeat one'), findsOneWidget);
      expect(find.byIcon(Icons.repeat_one), findsOneWidget);
      expect(find.byIcon(Icons.repeat), findsNothing);

      await tester.tap(find.byTooltip('Repeat one'));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Repeat off'), findsOneWidget);
      expect(_button(tester, 'Repeat off').isSelected, isFalse);

      expect(client.setModesCalls.map((c) => c.repeat), [
        RepeatSetting.all,
        RepeatSetting.one,
        RepeatSetting.off,
      ]);
    });

    _test('follow a change made on the laptop', (tester) async {
      final live = _Live(FakeApiClient());
      await live.pump(tester);
      expect(find.byTooltip('Shuffle off'), findsOneWidget);

      await live.push(
        tester,
        wsFrame('modes', {'shuffle': true, 'repeat': 'one'}),
      );

      expect(find.byTooltip('Shuffle on'), findsOneWidget);
      expect(find.byTooltip('Repeat one'), findsOneWidget);
      live.dispose();
    });

    _test('501 turns off only these two buttons, with a note', (tester) async {
      final client = FakeApiClient()..modesError = _notSupported;
      await _pump(tester, client);

      expect(find.text('Not available on this laptop'), findsOneWidget);
      expect(_button(tester, 'Shuffle off').onPressed, isNull);
      expect(_button(tester, 'Repeat off').onPressed, isNull);
      // Everything else works.
      expect(_button(tester, 'Play / pause').onPressed, isNotNull);
      expect(_button(tester, 'Next').onPressed, isNotNull);
      expect(find.text('Open Apple Music on your laptop'), findsNothing);
    });

    _test('a failed change says so and the buttons come back', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);
      client.modesError = const ApiException('INTERNAL', 'did not change', 500);

      await tester.tap(find.byTooltip('Shuffle off'));
      await tester.pumpAndSettle();

      expect(find.text("Couldn't change that on your laptop"), findsOneWidget);
      expect(_button(tester, 'Shuffle off').onPressed, isNotNull);
    });
  });

  group('the Up Next list', () {
    _test('shows the next songs with a note and an AutoPlay divider', (
      tester,
    ) async {
      final client = FakeApiClient();
      await _pump(tester, client);

      expect(find.text('Up Next'), findsOneWidget);
      expect(find.text('Shows about the next 20 songs'), findsOneWidget);
      await tester.dragUntilVisible(
        find.text('Auto Song 2'),
        find.byType(Scrollable).first,
        const Offset(0, -150),
      );
      expect(find.text('Queue Song 0'), findsOneWidget);
      expect(find.text('Artist 0'), findsOneWidget);
      expect(find.text('Queue Song 1'), findsOneWidget);
      expect(find.text('Auto Song 2'), findsOneWidget);
      expect(find.text('AutoPlay'), findsOneWidget, reason: 'one divider');
      expect(find.text('3:20'), findsOneWidget, reason: 'duration of row 0');
    });

    _test('is read once when the Player screen opens', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);

      expect(client.getQueueCalls, 1);
    });

    _test('scrolls to the last of many rows', (tester) async {
      final client = FakeApiClient()
        ..queue = [
          for (var i = 0; i < 30; i++)
            QueueItem(index: i, title: 'Row $i', artist: 'A'),
        ];
      await _pump(tester, client);
      // The rows are built, but the last one is far below the screen.
      expect(find.text('Row 29').hitTestable(), findsNothing);

      await tester.drag(find.byType(Scrollable).first, const Offset(0, -4000));
      await tester.pumpAndSettle();

      expect(find.text('Row 29').hitTestable(), findsOneWidget);
      expect(
        find.text('Row 0').hitTestable(),
        findsNothing,
        reason: 'scrolled',
      );
    });

    _test('tapping a row plays it by index and title, and shows a busy '
        'mark', (tester) async {
      final client = FakeApiClient()..queuePlayGate = Completer<void>();
      await _pump(tester, client);

      await _tapVisible(tester, find.text('Queue Song 1'));
      await tester.pump();

      expect(client.playedQueue.single, (index: 1, title: 'Queue Song 1'));
      expect(
        find.descendant(
          of: find.widgetWithText(ListTile, 'Queue Song 1'),
          matching: find.byType(CircularProgressIndicator),
        ),
        findsOneWidget,
      );
      expect(
        tester
            .widget<ListTile>(find.widgetWithText(ListTile, 'Queue Song 0'))
            .enabled,
        isFalse,
        reason: 'one at a time',
      );

      client.queuePlayGate!.complete();
      await tester.pumpAndSettle();
      expect(
        find.byTooltip('Play / pause'),
        findsOneWidget,
        reason: 'still on Player',
      );
    });

    _test('404 says the queue changed and reads the list again', (
      tester,
    ) async {
      final client = FakeApiClient();
      await _pump(tester, client);
      client.queuePlayError = const ApiException('NOT_FOUND', 'moved', 404);
      client.queue = const [QueueItem(index: 0, title: 'After The Move')];

      await _tapVisible(tester, find.text('Queue Song 1'));
      await tester.pumpAndSettle();

      expect(find.text('The queue changed'), findsOneWidget);
      expect(find.text('After The Move'), findsOneWidget);
      expect(find.text('Queue Song 1'), findsNothing);
    });

    _test('a row that cannot be played from the phone says so', (tester) async {
      final client = FakeApiClient()..queuePlayError = _notSupported;
      await _pump(tester, client);

      await _tapVisible(tester, find.text('Queue Song 0'));
      await tester.pumpAndSettle();

      expect(
        find.text("This song can't be played from the phone"),
        findsOneWidget,
      );
    });

    _test('another failure says so, with the reason', (tester) async {
      final client = FakeApiClient()
        ..queuePlayError = const ApiException('INTERNAL', 'wrong song', 500);
      await _pump(tester, client);

      await _tapVisible(tester, find.text('Queue Song 0'));
      await tester.pumpAndSettle();

      expect(find.text("Couldn't play that song: wrong song"), findsOneWidget);
    });

    _test('is read again when the laptop moves to the next song', (
      tester,
    ) async {
      final client = FakeApiClient();
      final live = _Live(client);
      await live.pump(tester);
      final reads = client.getQueueCalls;
      client.queue = const [QueueItem(index: 0, title: 'The Song After')];

      await live.push(
        tester,
        wsStateFrame(title: 'Now Playing', artworkRev: 9),
      );

      expect(client.getQueueCalls, reads + 1);
      expect(find.text('The Song After'), findsOneWidget);
      expect(find.text('Queue Song 0'), findsNothing);
      live.dispose();
    });

    _test('pull to refresh reads it again', (tester) async {
      final client = FakeApiClient();
      await _pump(tester, client);
      client.queue = const [QueueItem(index: 0, title: 'Fresh Row')];

      await tester.fling(
        find.byType(ListView).first,
        const Offset(0, 400),
        1000,
      );
      await tester.pumpAndSettle();

      expect(client.getQueueCalls, 2);
      expect(find.text('Fresh Row'), findsOneWidget);
    });

    _test('501 says it is not available, without an error', (tester) async {
      final client = FakeApiClient()..queueError = _notSupported;
      await _pump(tester, client);

      expect(find.text("Queue isn't available on this laptop"), findsOneWidget);
      expect(find.textContaining("Couldn't"), findsNothing);
      expect(find.byTooltip('Play / pause'), findsOneWidget);
    });

    _test('an error shows Retry, and Retry reads it again', (tester) async {
      final client = FakeApiClient()
        ..queueError = const AgentUnreachableException();
      await _pump(tester, client);
      expect(find.text("Couldn't load the queue"), findsOneWidget);

      client.queueError = null;
      await _tapVisible(tester, find.text('Retry'));
      await tester.pumpAndSettle();

      expect(find.text('Queue Song 0'), findsOneWidget);
    });

    _test('an empty queue says so', (tester) async {
      final client = FakeApiClient()..queue = const [];
      await _pump(tester, client);

      expect(find.text('Nothing is queued'), findsOneWidget);
    });
  });

  testWidgets('on a short phone the transport buttons stay on screen, with the '
      'new rows below them', (tester) async {
    tester.view.devicePixelRatio = 1.0;
    tester.view.physicalSize = const Size(360, 640);
    addTearDown(tester.view.reset);
    await _pump(tester, FakeApiClient());

    // Still usable at once, without scrolling.
    expect(find.byTooltip('Previous').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Play / pause').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Next').hitTestable(), findsOneWidget);
    expect(find.byTooltip('Mute').hitTestable(), findsOneWidget);
    // Shuffle and repeat, and the list, are a scroll away.
    await tester.ensureVisible(find.byTooltip('Shuffle off'));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Shuffle off').hitTestable(), findsOneWidget);
  });

  _test('409 shows the usual "Open Apple Music" state, and Retry reads '
      'the buttons and the queue again', (tester) async {
    final client = FakeApiClient()
      ..queueError = _notRunning
      ..modesError = _notRunning;
    await _pump(tester, client);

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
    expect(find.byTooltip('Shuffle off'), findsNothing);
    expect(find.text('Up Next'), findsNothing);

    client
      ..queueError = null
      ..modesError = null;
    final modeReads = client.getModesCalls;
    final queueReads = client.getQueueCalls;
    await _tapVisible(tester, find.text('Retry'));
    await tester.pumpAndSettle();

    expect(client.getModesCalls, greaterThan(modeReads));
    expect(client.getQueueCalls, greaterThan(queueReads));
    expect(find.text('Open Apple Music on your laptop'), findsNothing);
    expect(find.byTooltip('Shuffle off'), findsOneWidget);
    expect(find.text('Queue Song 0'), findsOneWidget);
  });
}
