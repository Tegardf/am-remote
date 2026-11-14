import 'dart:async';

import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/agent_status.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

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

const _notRunning = ApiException(
  'APPLE_MUSIC_NOT_RUNNING',
  'Apple Music is not running',
  409,
);

void main() {
  testWidgets(
    'NO_AUDIO_SESSION keeps the player and switches off only the volume',
    (tester) async {
      final client = FakeApiClient()
        ..volumeError = const ApiException(
          'NO_AUDIO_SESSION',
          'Apple Music has no audio session yet',
          409,
        );
      await _pump(tester, client);

      // Not the "Open Apple Music" screen: the buttons and the track stay.
      expect(find.text('Open Apple Music on your laptop'), findsNothing);
      expect(find.byTooltip('Play / pause'), findsOneWidget);
      expect(
        find.text('Volume works once Apple Music has played something'),
        findsOneWidget,
      );

      // Only the volume controls are off.
      // (The progress bar is a Slider too: the volume one has 100 divisions.)
      final volume = find.byWidgetPredicate(
        (w) => w is Slider && w.divisions == 100,
      );
      expect(tester.widget<Slider>(volume).onChanged, isNull);
      final mute = tester.widget<IconButton>(
        find.ancestor(
          of: find.byTooltip('Mute'),
          matching: find.byType(IconButton),
        ),
      );
      expect(mute.onPressed, isNull);

      await tester.tap(find.byTooltip('Play / pause'));
      await tester.pumpAndSettle();
      expect(client.calls, ['toggle']);
    },
  );

  testWidgets('APPLE_MUSIC_NOT_RUNNING on the volume still shows the empty '
      'state', (tester) async {
    final client = FakeApiClient()..volumeError = _notRunning;
    await _pump(tester, client);

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
    expect(
      find.text('Volume works once Apple Music has played something'),
      findsNothing,
    );
  });

  testWidgets('shows the volume loaded from the laptop', (tester) async {
    await _pump(tester, FakeApiClient());
    expect(find.text('42'), findsOneWidget);
    expect(find.byIcon(Icons.volume_up), findsOneWidget);
  });

  testWidgets('each button calls its API method once', (tester) async {
    final client = FakeApiClient();
    await _pump(tester, client);

    await tester.tap(find.byTooltip('Previous'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Play / pause'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Next'));
    await tester.pumpAndSettle();

    expect(client.calls, ['previous', 'toggle', 'next']);
  });

  testWidgets('mute toggles through POST /volume and flips the icon', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);

    await tester.tap(find.byTooltip('Mute'));
    await tester.pumpAndSettle();

    expect(client.setVolumeCalls.single, (level: null, muted: true));
    expect(find.byTooltip('Unmute'), findsOneWidget);
    expect(find.byIcon(Icons.volume_off), findsOneWidget);
  });

  testWidgets('buttons are disabled while a request is in flight', (
    tester,
  ) async {
    final client = FakeApiClient()..gate = Completer<void>();
    await _pump(tester, client);

    await tester.tap(find.byTooltip('Play / pause'));
    await tester.pump();

    IconButton button(String tooltip) => tester.widget<IconButton>(
      find.ancestor(
        of: find.byTooltip(tooltip),
        matching: find.byType(IconButton),
      ),
    );
    expect(button('Next').onPressed, isNull);
    expect(button('Previous').onPressed, isNull);
    expect(button('Play / pause').onPressed, isNull);

    client.gate!.complete();
    await tester.pumpAndSettle();
    expect(button('Next').onPressed, isNotNull);
    expect(client.calls, ['toggle']);
  });

  testWidgets('409 shows the empty state and Retry asks the laptop again', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);

    client.commandError = _notRunning;
    await tester.tap(find.byTooltip('Play / pause'));
    await tester.pumpAndSettle();

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
    expect(find.byTooltip('Play / pause'), findsNothing);

    client.commandError = null;
    final volumeCallsBefore = client.getVolumeCalls;
    final statusCallsBefore = client.statusCalls;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();

    expect(client.getVolumeCalls, volumeCallsBefore + 1);
    expect(client.statusCalls, statusCallsBefore + 1);
    expect(find.text('Open Apple Music on your laptop'), findsNothing);
    expect(find.byTooltip('Play / pause'), findsOneWidget);
  });

  testWidgets('Apple Music not running per /status shows the empty state', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..status = const AgentStatus(
        agentVersion: '0.1.0',
        appleMusicRunning: false,
      );
    await _pump(tester, client);

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
  });

  testWidgets('an unreachable laptop shows a snackbar and re-enables buttons', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);

    client.commandError = const AgentUnreachableException();
    await tester.tap(find.byTooltip('Next'));
    await tester.pumpAndSettle();

    expect(find.text("Can't reach the laptop"), findsOneWidget);
    final next = tester.widget<IconButton>(
      find.ancestor(
        of: find.byTooltip('Next'),
        matching: find.byType(IconButton),
      ),
    );
    expect(next.onPressed, isNotNull);
  });
}
