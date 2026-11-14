import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/playlist.dart';
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

/// Taps a tab in the bottom navigation bar.
Future<void> _openTab(WidgetTester tester, String label) async {
  await tester.tap(
    find.descendant(of: find.byType(NavigationBar), matching: find.text(label)),
  );
  await tester.pumpAndSettle();
}

const _notSupported = ApiException(
  'NOT_SUPPORTED',
  'Playlists are not available on this machine',
  501,
);
const _notRunning = ApiException(
  'APPLE_MUSIC_NOT_RUNNING',
  'Apple Music is not running',
  409,
);

void main() {
  testWidgets('the app has Player, Playlists and Search tabs, Player first', (
    tester,
  ) async {
    await _pump(tester, FakeApiClient());

    final bar = find.byType(NavigationBar);
    for (final label in ['Player', 'Playlists', 'Search']) {
      expect(
        find.descendant(of: bar, matching: find.text(label)),
        findsOneWidget,
      );
    }
    expect(find.byTooltip('Play / pause'), findsOneWidget);
  });

  testWidgets('playlists are only asked for once the tab is opened', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    expect(client.getPlaylistsCalls, 0);

    await _openTab(tester, 'Playlists');

    expect(client.getPlaylistsCalls, 1);
    expect(find.text('Favourites'), findsOneWidget);
    expect(find.text('Road Trip'), findsOneWidget);
  });

  testWidgets('pull to refresh reads the playlists again', (tester) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    client.playlists = const [];
    await tester.fling(find.byType(ListView), const Offset(0, 400), 1000);
    await tester.pumpAndSettle();

    expect(client.getPlaylistsCalls, 2);
    expect(find.text('No playlists yet'), findsOneWidget);
  });

  testWidgets('tapping a playlist plays it and switches to the Player tab', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    await tester.tap(find.text('Road Trip'));
    await tester.pumpAndSettle();

    expect(client.playedPlaylists, ['pl-2']);
    expect(find.byTooltip('Play / pause'), findsOneWidget);
    expect(
      find.text('Favourites'),
      findsNothing,
      reason: 'Player tab is shown',
    );
  });

  testWidgets('NOT_SUPPORTED shows a friendly explanation, not an error', (
    tester,
  ) async {
    final client = FakeApiClient()..playlistsError = _notSupported;
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    expect(
      find.text("Playlists aren't available on this laptop yet"),
      findsOneWidget,
    );
    expect(find.textContaining("Couldn't"), findsNothing);
    expect(find.text('Retry'), findsNothing);
  });

  testWidgets('Apple Music closed shows the empty state, and Retry reloads', (
    tester,
  ) async {
    final client = FakeApiClient()..playlistsError = _notRunning;
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');
    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);

    client.playlistsError = null;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();

    expect(find.text('Favourites'), findsOneWidget);
    expect(client.getPlaylistsCalls, 2);
  });

  testWidgets('another failure shows "Couldn\'t load" with Retry', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..playlistsError = const AgentUnreachableException();
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');
    expect(find.text("Couldn't load your playlists"), findsOneWidget);

    client.playlistsError = null;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();

    expect(find.text('Favourites'), findsOneWidget);
  });

  testWidgets('a malformed answer is an error state, not a crash', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..playlistsError = const FormatException('Expected a JSON array');
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    expect(find.text("Couldn't load your playlists"), findsOneWidget);
  });

  testWidgets('a playlist that fails to play shows a message and stays', (
    tester,
  ) async {
    final client = FakeApiClient()..playError = _notSupported;
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    await tester.tap(find.text('Favourites'));
    await tester.pumpAndSettle();

    expect(find.text("Your laptop can't play playlists yet"), findsOneWidget);
    expect(
      find.text('Favourites'),
      findsOneWidget,
      reason: 'still on the list',
    );
  });

  testWidgets(
    'a playlist that fails for another reason says so in plain words',
    (tester) async {
      final client = FakeApiClient()
        ..playError = const AgentUnreachableException();
      await _pump(tester, client);
      await _openTab(tester, 'Playlists');

      await tester.tap(find.text('Favourites'));
      await tester.pumpAndSettle();

      expect(
        find.text(
          "Couldn't play that playlist: The app couldn't reach the laptop",
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets('a failed load says why, in the words of the failure', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..playlistsError = const AgentUnreachableException('receiveTimeout');
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    expect(find.text("Couldn't load your playlists"), findsOneWidget);
    expect(find.text('The laptop took too long to answer'), findsOneWidget);
  });

  testWidgets("a failed load shows the agent's own message", (tester) async {
    final client = FakeApiClient()
      ..playlistsError = const ApiException(
        'INTERNAL',
        'the page "Road Trip" did not open in Apple Music in time',
        500,
      );
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    expect(
      find.text('the page "Road Trip" did not open in Apple Music in time'),
      findsOneWidget,
    );
  });

  testWidgets('a playlist that is gone says so and the list is read again', (
    tester,
  ) async {
    final client = FakeApiClient()
      ..playError = const ApiException('NOT_FOUND', 'no such id', 404);
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    client.playlists = const [Playlist(id: 'pl-3', name: 'New One')];
    await tester.tap(find.text('Favourites'));
    await tester.pumpAndSettle();

    expect(find.text('That playlist is no longer available'), findsOneWidget);
    expect(find.text('New One'), findsOneWidget, reason: 'the list was reread');
    expect(find.text('Favourites'), findsNothing);
  });

  testWidgets('a refresh that fails keeps the list and tells you', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    client.playlistsError = const AgentUnreachableException();
    await tester.fling(find.byType(ListView), const Offset(0, 400), 1000);
    await tester.pumpAndSettle();

    expect(find.text("Couldn't refresh your playlists"), findsOneWidget);
    expect(find.text('Favourites'), findsOneWidget, reason: 'the list stays');
    expect(find.text("Couldn't load your playlists"), findsNothing);
  });

  testWidgets('Apple Music closed when playing says to open it', (
    tester,
  ) async {
    final client = FakeApiClient()..playError = _notRunning;
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');

    await tester.tap(find.text('Favourites'));
    await tester.pumpAndSettle();

    expect(find.text('Open Apple Music on your laptop'), findsOneWidget);
    expect(find.text('Favourites'), findsOneWidget);
  });

  testWidgets('the list is kept when you switch tabs and come back', (
    tester,
  ) async {
    final client = FakeApiClient();
    await _pump(tester, client);
    await _openTab(tester, 'Playlists');
    await _openTab(tester, 'Player');
    await _openTab(tester, 'Playlists');

    expect(find.text('Favourites'), findsOneWidget);
    expect(client.getPlaylistsCalls, 1, reason: 'not read again');
  });
}
