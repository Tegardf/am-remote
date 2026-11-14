import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/features/connect/pairing_screen.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/fakes.dart';

Future<void> _pump(
  WidgetTester tester, {
  required FakeApiClient client,
  InMemorySessionStorage? storage,
}) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: testOverrides(client: client, storage: storage),
      child: const MaterialApp(home: PairingScreen(agent: testAgent)),
    ),
  );
  await tester.pumpAndSettle();
}

Finder get _codeField => find.widgetWithText(TextFormField, 'Pairing code');

void main() {
  testWidgets('defaults the device name to the phone model', (tester) async {
    await _pump(tester, client: FakeApiClient());
    expect(find.widgetWithText(TextFormField, 'Test Phone'), findsOneWidget);
  });

  testWidgets('rejects a code that is not 6 digits', (tester) async {
    final client = FakeApiClient();
    await _pump(tester, client: client);

    await tester.enterText(_codeField, '12345');
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(find.text('Enter all 6 digits'), findsOneWidget);
    expect(client.pairCalls, isEmpty);
  });

  testWidgets('only accepts digits', (tester) async {
    await _pump(tester, client: FakeApiClient());

    await tester.enterText(_codeField, 'ab1c2-3');
    await tester.pump();

    expect(find.widgetWithText(TextFormField, '123'), findsOneWidget);
  });

  testWidgets('BAD_PAIRING_CODE shows "Wrong or expired code"', (tester) async {
    final client = FakeApiClient()
      ..pairError = const ApiException(
        'BAD_PAIRING_CODE',
        'Pairing code is wrong or expired',
        403,
      );
    final storage = InMemorySessionStorage();
    await _pump(tester, client: client, storage: storage);

    await tester.enterText(_codeField, '123456');
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(find.text('Wrong or expired code'), findsOneWidget);
    expect(client.pairCalls, hasLength(1));
    expect(client.pairCalls.single.code, '123456');
    expect(client.pairCalls.single.deviceName, 'Test Phone');
    expect(storage.token, isNull);
  });

  testWidgets('an unreachable laptop says so', (tester) async {
    final client = FakeApiClient()
      ..pairError = const AgentUnreachableException();
    await _pump(tester, client: client);

    await tester.enterText(_codeField, '123456');
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(
      find.textContaining("Can't reach 192.168.1.20:8765"),
      findsOneWidget,
    );
  });

  testWidgets('a correct code stores the token and the host', (tester) async {
    final client = FakeApiClient();
    final storage = InMemorySessionStorage();
    await _pump(tester, client: client, storage: storage);

    await tester.enterText(_codeField, '123456');
    await tester.tap(find.text('Pair'));
    await tester.pumpAndSettle();

    expect(storage.token, fakeToken);
    expect(storage.agent, testAgent);
  });
}
