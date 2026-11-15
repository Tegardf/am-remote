import 'dart:async';

import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/app_search.dart';
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

Future<void> _openTab(WidgetTester tester, String label) async {
  await tester.tap(
    find.descendant(of: find.byType(NavigationBar), matching: find.text(label)),
  );
  await tester.pumpAndSettle();
}

/// Types into the search box and waits past the 300 ms pause.
Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(find.byType(TextField), text);
  await tester.pump(const Duration(milliseconds: 350));
  await tester.pumpAndSettle();
}

Future<void> _pumpSearch(WidgetTester tester, FakeApiClient client) async {
  await _pump(tester, client);
  await _openTab(tester, 'Search');
}

void main() {
  testWidgets('shows the hint and the side-effect note before anything is '
      'typed', (tester) async {
    await _pumpSearch(tester, FakeApiClient());

    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
    expect(
      find.text(
        'Searching opens the search page in Apple Music on the laptop.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('Catalog is selected first, and Library is one tap away', (
    tester,
  ) async {
    await _pumpSearch(tester, FakeApiClient());

    final switcher = tester.widget<SegmentedButton<SearchScope>>(
      find.byType(SegmentedButton<SearchScope>),
    );
    expect(switcher.selected, {SearchScope.catalog});
    expect(find.text('Catalog'), findsOneWidget);
    expect(find.text('Library'), findsOneWidget);
  });

  testWidgets('typing waits for a pause, then shows the results grouped by '
      'section, with no pictures', (tester) async {
    final client = FakeApiClient();
    await _pumpSearch(tester, client);

    await tester.enterText(find.byType(TextField), 'ex');
    await tester.pump(const Duration(milliseconds: 200));
    expect(client.appSearchCalls, isEmpty, reason: 'still inside the 300 ms');

    await tester.pump(const Duration(milliseconds: 150));
    await tester.pumpAndSettle();
    expect(client.appSearchCalls, [(q: 'ex', scope: SearchScope.catalog)]);

    // Section headings, then title and subtitle.
    expect(find.text('Songs'), findsOneWidget);
    expect(find.text('Albums'), findsOneWidget);
    expect(find.text('Example Song'), findsOneWidget);
    expect(find.text('Second Song'), findsOneWidget);
    expect(find.text('Example Album'), findsOneWidget);
    expect(find.text('Example Artist'), findsNWidgets(3));
    expect(find.byType(Image), findsNothing);
  });

  testWidgets('typing "ab" quickly sends one request and "a" none', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pumpSearch(tester, client);

    await tester.enterText(find.byType(TextField), 'a');
    await tester.pump(const Duration(milliseconds: 100));
    await tester.enterText(find.byType(TextField), 'ab');
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(client.appSearchCalls.map((s) => s.q), ['ab']);

    client.appSearchCalls.clear();
    await tester.enterText(find.byType(TextField), 'a');
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(client.appSearchCalls, isEmpty);
    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
  });

  testWidgets('the Library switch searches again with the library scope', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pumpSearch(tester, client);
    await _type(tester, 'tulus');

    await tester.tap(find.text('Library'));
    await tester.pumpAndSettle();

    expect(client.appSearchCalls, [
      (q: 'tulus', scope: SearchScope.catalog),
      (q: 'tulus', scope: SearchScope.library),
    ]);
    expect(find.text('Example Song'), findsOneWidget);
  });

  testWidgets('a slow search shows a clear loading state until it answers', (
    tester,
  ) async {
    final client = FakeApiClient()..appSearchGate = Completer<void>();
    await _pumpSearch(tester, client);

    await tester.enterText(find.byType(TextField), 'ex');
    await tester.pump(const Duration(milliseconds: 350));
    // A long wait (a real search takes about 4 s, and may take much longer).
    await tester.pump(const Duration(seconds: 30));

    expect(find.text('Searching in Apple Music…'), findsOneWidget);
    expect(find.byType(LinearProgressIndicator), findsOneWidget);
    expect(find.text('Example Song'), findsNothing);

    client.appSearchGate!.complete();
    await tester.pumpAndSettle();
    expect(find.text('Searching in Apple Music…'), findsNothing);
    expect(find.text('Example Song'), findsOneWidget);
  });

  testWidgets('tapping a result sends q, scope, section, index and title and '
      'shows the Player tab', (tester) async {
    final client = FakeApiClient();
    await _pumpSearch(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');
    await tester.tap(find.text('Library'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Second Song'));
    await tester.pumpAndSettle();

    expect(client.playedAppResults.single, (
      q: 'ex',
      scope: SearchScope.library,
      section: 'Songs',
      index: 1,
      title: 'Second Song',
    ));
    expect(find.byTooltip('Play / pause'), findsOneWidget);
    expect(find.text('Second Song'), findsNothing, reason: 'Player tab');
  });

  testWidgets('a result that is being played shows a busy mark and the others '
      'are off', (tester) async {
    final client = FakeApiClient()..appPlayGate = Completer<void>();
    await _pumpSearch(tester, client);
    await _type(tester, 'ex');

    await tester.tap(find.text('Example Song'));
    await tester.pump();

    expect(
      find.descendant(
        of: find.widgetWithText(ListTile, 'Example Song'),
        matching: find.byType(CircularProgressIndicator),
      ),
      findsOneWidget,
    );
    expect(
      tester
          .widget<ListTile>(find.widgetWithText(ListTile, 'Second Song'))
          .enabled,
      isFalse,
    );

    client.appPlayGate!.complete();
    await tester.pumpAndSettle();
  });

  group('failures', () {
    testWidgets('501 on search explains it, without an error', (tester) async {
      final client = FakeApiClient()
        ..appSearchError = const ApiException('NOT_SUPPORTED', 'no', 501);
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');

      expect(
        find.text("Search isn't available on this laptop yet"),
        findsOneWidget,
      );
      expect(find.textContaining("Couldn't"), findsNothing);
    });

    testWidgets('an agent without the route (404) says the same', (
      tester,
    ) async {
      final client = FakeApiClient()
        ..appSearchError = const ApiException(
          'NOT_FOUND',
          'No such route',
          404,
        );
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');

      expect(
        find.text("Search isn't available on this laptop yet"),
        findsOneWidget,
      );
    });

    testWidgets('Apple Music closed shows the empty state and Retry searches '
        'again', (tester) async {
      final client = FakeApiClient()
        ..appSearchError = const ApiException(
          'APPLE_MUSIC_NOT_RUNNING',
          'closed',
          409,
        );
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');
      expect(find.text('Open Apple Music on your laptop'), findsOneWidget);

      client.appSearchError = null;
      await tester.tap(find.text('Retry'));
      await tester.pumpAndSettle();
      expect(find.text('Example Song'), findsOneWidget);
    });

    testWidgets('another error says so and Retry searches again', (
      tester,
    ) async {
      final client = FakeApiClient()
        ..appSearchError = const ApiException('INTERNAL', 'timeout', 500);
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');
      expect(
        find.text("Couldn't search. Check the connection to your laptop."),
        findsOneWidget,
      );

      client.appSearchError = null;
      await tester.tap(find.text('Retry'));
      await tester.pumpAndSettle();
      expect(find.text('Example Song'), findsOneWidget);
    });

    testWidgets('no match says so', (tester) async {
      final client = FakeApiClient()..appResults = const [];
      await _pumpSearch(tester, client);
      await _type(tester, 'zzzzqqqq');

      expect(find.text('No results for "zzzzqqqq"'), findsOneWidget);
    });

    testWidgets(
      'a kind the agent cannot play (501) gets a friendly message and '
      'stays on Search',
      (tester) async {
        final client = FakeApiClient()
          ..appPlayError = const ApiException('NOT_SUPPORTED', 'artist', 501);
        await _pumpSearch(tester, client);
        await _type(tester, 'ex');

        await tester.tap(find.text('Example Song'));
        await tester.pumpAndSettle();

        expect(
          find.text("This kind of result can't be played from the phone yet"),
          findsOneWidget,
        );
        expect(
          find.text('Example Song'),
          findsOneWidget,
          reason: 'still on Search',
        );
      },
    );

    testWidgets(
      'a result the search no longer finds (404) says so and searches '
      'again',
      (tester) async {
        final client = FakeApiClient()
          ..appPlayError = const ApiException('NOT_FOUND', 'moved', 404);
        await _pumpSearch(tester, client);
        await _type(tester, 'ex');

        await tester.tap(find.text('Example Song'));
        await tester.pumpAndSettle();

        expect(
          find.text("That result isn't there any more. Searching again"),
          findsOneWidget,
        );
        expect(client.appSearchCalls, hasLength(2));
      },
    );

    testWidgets('Apple Music closed when playing says to open it', (
      tester,
    ) async {
      final client = FakeApiClient()
        ..appPlayError = const ApiException(
          'APPLE_MUSIC_NOT_RUNNING',
          'closed',
          409,
        );
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');

      await tester.tap(find.text('Example Song'));
      await tester.pumpAndSettle();

      expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
    });

    testWidgets('another failure while playing says so with the reason', (
      tester,
    ) async {
      final client = FakeApiClient()
        ..appPlayError = const ApiException('INTERNAL', 'it timed out', 500);
      await _pumpSearch(tester, client);
      await _type(tester, 'ex');

      await tester.tap(find.text('Example Song'));
      await tester.pumpAndSettle();

      expect(find.text("Couldn't play that: it timed out"), findsOneWidget);
    });
  });

  testWidgets('the text, the scope and the results are kept when you switch '
      'tabs', (tester) async {
    final client = FakeApiClient();
    await _pumpSearch(tester, client);
    await _type(tester, 'ex');
    await tester.tap(find.text('Library'));
    await tester.pumpAndSettle();
    final searches = client.appSearchCalls.length;

    await _openTab(tester, 'Player');
    await _openTab(tester, 'Search');

    expect(find.text('Example Song'), findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      'ex',
    );
    expect(
      tester
          .widget<SegmentedButton<SearchScope>>(
            find.byType(SegmentedButton<SearchScope>),
          )
          .selected,
      {SearchScope.library},
    );
    expect(client.appSearchCalls, hasLength(searches), reason: 'not again');
  });

  testWidgets('the search box takes at most 200 characters', (tester) async {
    await _pumpSearch(tester, FakeApiClient());

    await tester.enterText(find.byType(TextField), 'x' * 250);
    await tester.pump();

    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.controller!.text.length, 200);
  });

  testWidgets('the clear button empties the box and the results', (
    tester,
  ) async {
    await _pumpSearch(tester, FakeApiClient());
    await _type(tester, 'ex');
    expect(find.text('Example Song'), findsOneWidget);

    await tester.tap(find.byTooltip('Clear'));
    await tester.pumpAndSettle();

    expect(find.text('Example Song'), findsNothing);
    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      '',
    );
  });
}
