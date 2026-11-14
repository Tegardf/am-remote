import 'dart:convert';

import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/search_result.dart';
import 'package:am_remote/core/thumbnail_loader.dart';
import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

/// A valid 1x1 PNG.
final _png = base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
);

Future<void> _pump(
  WidgetTester tester,
  FakeApiClient client, {
  ThumbnailLoader? thumbnailLoader,
}) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: testOverrides(
        client: client,
        storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
        thumbnailLoader: thumbnailLoader,
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

const _tracks =
    'https://music.apple.com/us/album/example-album/1000000001?i=1000000002';

void main() {
  testWidgets('shows the hint before anything is typed', (tester) async {
    await _pump(tester, FakeApiClient());
    await _openTab(tester, 'Search');

    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
  });

  testWidgets('typing waits for a pause, then shows the results', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Search');

    await tester.enterText(find.byType(TextField), 'ex');
    await tester.pump(const Duration(milliseconds: 200));
    expect(client.searchCalls, isEmpty, reason: 'still inside the 300 ms');

    await tester.pump(const Duration(milliseconds: 150));
    await tester.pumpAndSettle();
    expect(client.searchCalls, [(query: 'ex', limit: 25)]);
    expect(find.text('Example Song'), findsOneWidget);
    expect(find.text('Example Artist · Example Album'), findsOneWidget);
  });

  testWidgets('typing "ab" quickly sends one request and "a" none', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Search');

    await tester.enterText(find.byType(TextField), 'a');
    await tester.pump(const Duration(milliseconds: 100));
    await tester.enterText(find.byType(TextField), 'ab');
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(client.searchCalls.map((s) => s.query), ['ab']);

    client.searchCalls.clear();
    await tester.enterText(find.byType(TextField), 'a');
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(client.searchCalls, isEmpty);
    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
  });

  testWidgets('tapping a result plays its url and shows the Player tab', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');

    await tester.tap(find.text('Example Song'));
    await tester.pumpAndSettle();

    expect(client.playedTracks, [_tracks]);
    expect(find.byTooltip('Play / pause'), findsOneWidget);
    expect(
      find.text('Example Song'),
      findsNothing,
      reason: 'Player tab is shown',
    );
  });

  testWidgets('the text and results are kept when you switch tabs', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');

    await _openTab(tester, 'Player');
    await _openTab(tester, 'Search');

    expect(find.widgetWithText(TextField, 'ex'), findsOneWidget);
    expect(find.text('Example Song'), findsOneWidget);
    expect(client.searchCalls, hasLength(1), reason: 'not searched again');
  });

  testWidgets('no results says so', (tester) async {
    final client = FakeApiClient()..searchResults = const [];
    await _pump(tester, client);
    await _openTab(tester, 'Search');

    await _type(tester, 'zzzz');

    expect(find.text('No results for "zzzz"'), findsOneWidget);
  });

  testWidgets('an error shows a message and Retry searches again', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..searchError = const AgentUnreachableException();
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');
    expect(find.textContaining("Couldn't search"), findsOneWidget);

    client.searchError = null;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();

    expect(find.text('Example Song'), findsOneWidget);
  });

  testWidgets('NOT_SUPPORTED explains that search is not available', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..searchError = const ApiException('NOT_SUPPORTED', 'no', 501);
    await _pump(tester, client);
    await _openTab(tester, 'Search');

    await _type(tester, 'ex');

    expect(
      find.text("Search isn't available on this laptop yet"),
      findsOneWidget,
    );
    expect(find.textContaining("Couldn't"), findsNothing);
  });

  testWidgets('Apple Music closed shows the empty state', (tester) async {
    final client = FakeApiClient()
      ..searchError = const ApiException('APPLE_MUSIC_NOT_RUNNING', 'x', 409);
    await _pump(tester, client);
    await _openTab(tester, 'Search');

    await _type(tester, 'ex');

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
  });

  testWidgets('a track that fails to play shows a message and stays', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..playError = const ApiException('INTERNAL', 'boom', 500);
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');

    await tester.tap(find.text('Example Song'));
    await tester.pumpAndSettle();

    expect(find.text("Couldn't play that track"), findsOneWidget);
    expect(
      find.text('Example Song'),
      findsOneWidget,
      reason: 'still on Search',
    );
  });

  testWidgets('a song the catalog no longer knows says so', (tester) async {
    final client = FakeApiClient()
      ..playError = const ApiException('NOT_FOUND', 'unknown song', 404);
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');

    await tester.tap(find.text('Example Song'));
    await tester.pumpAndSettle();

    expect(find.text('That track is no longer available'), findsOneWidget);
    expect(
      find.text('Example Song'),
      findsOneWidget,
      reason: 'still on Search',
    );
  });

  testWidgets('the search box takes at most 200 characters', (tester) async {
    await _pump(tester, FakeApiClient());
    await _openTab(tester, 'Search');

    await tester.enterText(find.byType(TextField), 'x' * 250);
    await tester.pump();

    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.controller!.text.length, 200);
  });

  testWidgets('the clear button empties the box and the results', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Search');
    await _type(tester, 'ex');

    await tester.tap(find.byTooltip('Clear'));
    await tester.pumpAndSettle();

    expect(find.text('Example Song'), findsNothing);
    expect(find.text('Type at least 2 characters to search'), findsOneWidget);
  });

  testWidgets(
    'only HTTPS artwork is loaded; anything else gets a placeholder',
    (tester) async {
      final client = FakeApiClient()
        ..searchResults = const [
          SearchResult(
            id: '1',
            title: 'Secure',
            artworkUrl: 'https://is1-ssl.mzstatic.com/a.jpg',
            url: 'https://music.apple.com/1',
          ),
          SearchResult(
            id: '2',
            title: 'Plain http',
            artworkUrl: 'http://example.com/a.jpg',
            url: 'https://music.apple.com/2',
          ),
          SearchResult(
            id: '3',
            title: 'No artwork',
            url: 'https://music.apple.com/3',
          ),
        ];
      // The real loader, over a fake network, so what it refuses is visible.
      final network = FakeHttpAdapter((_) => ResponseBody.fromBytes(_png, 200));
      await _pump(
        tester,
        client,
        thumbnailLoader: HttpsThumbnailLoader(
          dio: Dio()..httpClientAdapter = network,
        ),
      );
      await _openTab(tester, 'Search');

      await _type(tester, 'ex');

      // One picture request, for the HTTPS result only: the http:// one and
      // the empty one never reach the network.
      expect(network.requests.map((r) => r.uri.toString()), [
        'https://is1-ssl.mzstatic.com/a.jpg',
      ]);
      expect(find.byKey(const ValueKey('thumbnail')), findsOneWidget);
      expect(
        find.byKey(const ValueKey('thumbnail-placeholder')),
        findsNWidgets(2),
      );
    },
  );
}
