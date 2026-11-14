import 'package:am_remote/app_router.dart';
import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/discovery.dart';
import 'package:am_remote/features/connect/pairing_screen.dart';
import 'package:am_remote/features/player/player_screen.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

Future<void> _pump(
  WidgetTester tester, {
  FakeApiClient? client,
  InMemorySessionStorage? storage,
  FakeDiscovery? discovery,
}) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: testOverrides(
        client: client,
        storage: storage,
        discovery: discovery,
      ),
      child: const MaterialApp(home: AppRouter()),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('saved token + 200 → home with a "Connected" indicator', (
    tester,
  ) async {
    await _pump(
      tester,
      storage: InMemorySessionStorage(token: fakeToken, agent: testAgent),
    );

    expect(find.byType(PlayerScreen), findsOneWidget);
    expect(find.text('Connected'), findsOneWidget);
  });

  testWidgets('saved token + 401 → connect screen and the token is cleared', (
    tester,
  ) async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    await _pump(
      tester,
      client: FakeApiClient()
        ..statusError = const ApiException('UNAUTHORIZED', 'No', 401),
      storage: storage,
    );

    expect(find.text('Connect to your laptop'), findsOneWidget);
    expect(find.byType(PlayerScreen), findsNothing);
    expect(storage.token, isNull);
  });

  testWidgets('unreachable agent → home with an "Offline" indicator', (
    tester,
  ) async {
    final storage = InMemorySessionStorage(token: fakeToken, agent: testAgent);
    await _pump(
      tester,
      client: FakeApiClient()..statusError = const AgentUnreachableException(),
      storage: storage,
    );

    expect(find.byType(PlayerScreen), findsOneWidget);
    expect(find.text('Offline'), findsOneWidget);
    expect(storage.token, fakeToken);
  });

  testWidgets('discovered agents are listed and open the pairing screen', (
    tester,
  ) async {
    await _pump(
      tester,
      discovery: FakeDiscovery(const [
        DiscoveredAgent(
          name: 'LAPTOP',
          host: '192.168.1.20',
          port: 8765,
          version: '1',
        ),
      ]),
    );

    expect(find.text('LAPTOP'), findsOneWidget);
    await tester.tap(find.text('LAPTOP'));
    await tester.pumpAndSettle();

    expect(find.byType(PairingScreen), findsOneWidget);
  });

  testWidgets(
    'manual IP: invalid text is rejected, a valid one goes to pairing',
    (tester) async {
      await _pump(tester);

      await tester.tap(find.text('Enter IP manually'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'http://nope');
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Enter an address like'), findsOneWidget);
      expect(find.byType(PairingScreen), findsNothing);

      await tester.enterText(find.byType(TextField), '192.168.1.20');
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      expect(find.byType(PairingScreen), findsOneWidget);
    },
  );

  testWidgets('pairing from the connect screen lands on the home screen', (
    tester,
  ) async {
    final storage = InMemorySessionStorage();
    await _pump(tester, storage: storage);

    await tester.tap(find.text('Enter IP manually'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '192.168.1.20');
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextFormField, 'Pairing code'),
      '123456',
    );
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(find.byType(PlayerScreen), findsOneWidget);
    expect(find.byType(PairingScreen), findsNothing);
    expect(storage.token, fakeToken);
  });
}
