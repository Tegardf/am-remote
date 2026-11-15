import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/app_search.dart';
import 'package:am_remote/features/connect/session_controller.dart';
import 'package:am_remote/features/home/play_result.dart';
import 'package:am_remote/features/search/search_controller.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fakes.dart';

ProviderContainer _container(FakeApiClient client) {
  final container = ProviderContainer(
    overrides: [
      ...testOverrides(client: client),
      apiClientProvider.overrideWithValue(client),
    ],
  );
  addTearDown(container.dispose);
  container.listen(searchControllerProvider, (_, _) {});
  return container;
}

SearchState _state(ProviderContainer c) => c.read(searchControllerProvider);

SearchResultsController _controller(ProviderContainer c) =>
    c.read(searchControllerProvider.notifier);

const _debounce = Duration(milliseconds: 300);

AppSearchResult _result(
  String title, {
  String section = 'Songs',
  int index = 0,
}) => AppSearchResult(
  section: section,
  index: index,
  kind: 'song',
  title: title,
  subtitle: 'Artist',
);

void main() {
  test('a search text is trimmed and cut to 200 characters (longer is a '
      '400)', () {
    expect(SearchResultsController.queryToSend('  tulus  '), 'tulus');
    expect(SearchResultsController.queryToSend('a' * 200), 'a' * 200);
    expect(SearchResultsController.queryToSend('a' * 250), 'a' * 200);
    // Counted in characters, not UTF-16 units: an emoji is one.
    expect(
      SearchResultsController.queryToSend('😀' * 250).runes.length,
      SearchResultsController.maxQueryLength,
    );

    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);
      _controller(c).setQuery('b' * 250);
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(client.appSearchCalls.single.q, 'b' * 200);
    });
  });

  group('debounce', () {
    test('typing "a" then "ab" quickly sends one request, for "ab"', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);

        _controller(c).setQuery('a');
        async.elapse(const Duration(milliseconds: 100));
        _controller(c).setQuery('ab');
        async.elapse(const Duration(milliseconds: 299));
        expect(client.appSearchCalls, isEmpty, reason: 'still waiting');

        async.elapse(const Duration(milliseconds: 1));
        async.flushMicrotasks();
        expect(client.appSearchCalls, [(q: 'ab', scope: SearchScope.catalog)]);
        expect(_state(c).status, SearchStatus.ready);
      });
    });

    test('every keystroke restarts the 300 ms wait', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);

        for (final text in ['ab', 'abc', 'abcd', 'abcde']) {
          _controller(c).setQuery(text);
          async.elapse(const Duration(milliseconds: 200));
        }
        expect(client.appSearchCalls, isEmpty);

        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(client.appSearchCalls.map((s) => s.q), ['abcde']);
      });
    });

    test('one character, or spaces around one, searches nothing', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);

        _controller(c).setQuery('a');
        async.elapse(const Duration(seconds: 2));
        _controller(c).setQuery('  a  ');
        async.elapse(const Duration(seconds: 2));
        async.flushMicrotasks();

        expect(client.appSearchCalls, isEmpty);
        expect(_state(c).status, SearchStatus.idle);
      });
    });

    test('one emoji is one character, so it searches nothing', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);

        _controller(c).setQuery('😀');
        async.elapse(const Duration(seconds: 2));
        _controller(c).submit();
        async.flushMicrotasks();

        expect(client.appSearchCalls, isEmpty);
        expect(_state(c).status, SearchStatus.idle);

        _controller(c).setQuery('😀😀');
        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(client.appSearchCalls.map((s) => s.q), ['😀😀']);
      });
    });

    test('clearing the text forgets a search on its way', () {
      fakeAsync((async) {
        final client = FakeApiClient()..appSearchGate = Completer<void>();
        final c = _container(client);

        _controller(c).setQuery('ab');
        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.loading);

        _controller(c).setQuery('');
        client.appSearchGate!.complete();
        async.flushMicrotasks();

        expect(_state(c).status, SearchStatus.idle);
        expect(_state(c).results, isEmpty, reason: 'the old answer is dropped');
      });
    });

    test('the newest search wins, even if an older answer arrives last', () {
      fakeAsync((async) {
        final slow = Completer<void>();
        final fast = Completer<void>();
        final client = FakeApiClient()
          ..appResults = [_result('Old Answer')]
          ..appSearchGate = slow;
        final c = _container(client);

        _controller(c).setQuery('ab');
        async.elapse(_debounce);
        async.flushMicrotasks(); // the first search waits on its gate

        client
          ..appResults = [_result('New Answer')]
          ..appSearchGate = fast;
        _controller(c).setQuery('abc');
        async.elapse(_debounce);
        async.flushMicrotasks();
        fast.complete();
        async.flushMicrotasks();
        expect(_state(c).results.single.title, 'New Answer');

        slow.complete(); // the older answer arrives last
        async.flushMicrotasks();
        expect(_state(c).results.single.title, 'New Answer');
        expect(_state(c).searchedQuery, 'abc');
      });
    });
  });

  group('the Catalog / Library switch', () {
    test('defaults to Catalog', () {
      final c = _container(FakeApiClient());
      expect(_state(c).scope, SearchScope.catalog);
    });

    test('searches again at once with the new scope, and drops the old '
        'results', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);
        _controller(c).setQuery('tulus');
        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(_state(c).results, isNotEmpty);

        client.appSearchGate = Completer<void>();
        _controller(c).setScope(SearchScope.library);
        async.flushMicrotasks(); // no 300 ms wait: it is a tap

        expect(client.appSearchCalls, [
          (q: 'tulus', scope: SearchScope.catalog),
          (q: 'tulus', scope: SearchScope.library),
        ]);
        expect(_state(c).scope, SearchScope.library);
        expect(_state(c).results, isEmpty, reason: 'they were the catalog\'s');
      });
    });

    test('with nothing typed it only switches', () {
      final client = FakeApiClient();
      final c = _container(client);

      _controller(c).setScope(SearchScope.library);

      expect(_state(c).scope, SearchScope.library);
      expect(client.appSearchCalls, isEmpty);
      expect(_state(c).status, SearchStatus.idle);
    });

    test('the same scope again does nothing', () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);
        _controller(c).setQuery('ab');
        async.elapse(_debounce);
        async.flushMicrotasks();

        _controller(c).setScope(SearchScope.catalog);
        async.flushMicrotasks();

        expect(client.appSearchCalls, hasLength(1));
      });
    });
  });

  test('results are grouped by section, in the order they first appear', () {
    fakeAsync((async) {
      final client = FakeApiClient()
        ..appResults = [
          _result('Top Hit', section: 'Top Results'),
          _result('Song A'),
          _result('Album A', section: 'Albums'),
          _result('Song B', index: 1),
        ];
      final c = _container(client);
      _controller(c).setQuery('ab');
      async.elapse(_debounce);
      async.flushMicrotasks();

      final sections = _state(c).sections;
      expect(sections.map((s) => s.section), [
        'Top Results',
        'Songs',
        'Albums',
      ]);
      expect(sections[1].results.map((r) => r.title), ['Song A', 'Song B']);
    });
  });

  group('statuses', () {
    test('an empty answer is "ready" with nothing in it', () {
      fakeAsync((async) {
        final client = FakeApiClient()..appResults = const [];
        final c = _container(client);
        _controller(c).setQuery('zzzz');
        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.ready);
        expect(_state(c).results, isEmpty);
      });
    });

    test('501 and an agent without the route (404) mean "not supported"', () {
      for (final error in [
        const ApiException('NOT_SUPPORTED', 'no', 501),
        const ApiException('NOT_FOUND', 'No such route', 404),
      ]) {
        fakeAsync((async) {
          final client = FakeApiClient()..appSearchError = error;
          final c = _container(client);
          _controller(c).setQuery('ab');
          async.elapse(_debounce);
          async.flushMicrotasks();
          expect(_state(c).status, SearchStatus.notSupported);
        });
      }
    });

    test('409 means Apple Music is closed; anything else is an error', () {
      fakeAsync((async) {
        final client = FakeApiClient()
          ..appSearchError = const ApiException(
            'APPLE_MUSIC_NOT_RUNNING',
            'closed',
            409,
          );
        final c = _container(client);
        _controller(c).setQuery('ab');
        async.elapse(_debounce);
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.notRunning);

        client.appSearchError = const ApiException('INTERNAL', 'timeout', 500);
        _controller(c).retry();
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.error);

        client.appSearchError = const AgentUnreachableException();
        _controller(c).retry();
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.error);

        client.appSearchError = null;
        _controller(c).retry();
        async.flushMicrotasks();
        expect(_state(c).status, SearchStatus.ready);
      });
    });
  });

  group('playing a result', () {
    Future<ProviderContainer> searched(
      FakeApiClient client, {
      String text = 'tulus',
      SearchScope scope = SearchScope.catalog,
    }) async {
      final c = _container(client);
      _controller(c).setScope(scope);
      _controller(c).setQuery(text);
      _controller(c).submit();
      await pumpEventQueue();
      return c;
    }

    test('sends q, scope, section, index and title unchanged', () async {
      final client = FakeApiClient()
        ..appResults = [_result('Album Title', section: 'Albums', index: 3)];
      final c = await searched(client, scope: SearchScope.library);

      final outcome = await _controller(c).play(_state(c).results.single);

      expect(outcome, PlayResult.started);
      expect(client.playedAppResults.single, (
        q: 'tulus',
        scope: SearchScope.library,
        section: 'Albums',
        index: 3,
        title: 'Album Title',
      ));
      expect(_state(c).playingKey, isNull);
    });

    test('uses the search that produced the result, not the text typed '
        'since', () async {
      final client = FakeApiClient();
      final c = await searched(client, text: 'tulus');

      // The user starts typing something else; its search has not run yet.
      _controller(c).setQuery('another artist');
      await _controller(c).play(_state(c).results.first);

      expect(client.playedAppResults.single.q, 'tulus');
      expect(client.playedAppResults.single.scope, SearchScope.catalog);
    });

    test('a second tap while one is on its way is ignored', () async {
      final client = FakeApiClient()..appPlayGate = Completer<void>();
      final c = await searched(client);
      final results = _state(c).results;

      final first = _controller(c).play(results[0]);
      await pumpEventQueue();
      expect(_state(c).playingKey, resultKey(results[0]));

      expect(await _controller(c).play(results[1]), PlayResult.failed);
      expect(client.playedAppResults, hasLength(1));

      client.appPlayGate!.complete();
      expect(await first, PlayResult.started);
      expect(_state(c).playingKey, isNull);
    });

    test('clearing the text keeps a play on its way', () async {
      final client = FakeApiClient()..appPlayGate = Completer<void>();
      final c = await searched(client);

      final play = _controller(c).play(_state(c).results.first);
      await pumpEventQueue();
      _controller(c).setQuery('');

      expect(_state(c).playingKey, isNotNull, reason: 'still on its way');
      client.appPlayGate!.complete();
      await play;
      expect(_state(c).playingKey, isNull);
    });

    test('a play that fails reports why', () async {
      final client = FakeApiClient();
      final c = await searched(client);
      final result = _state(c).results.first;

      client.appPlayError = const ApiException('NOT_SUPPORTED', 'no', 501);
      expect(await _controller(c).play(result), PlayResult.notSupported);

      client.appPlayError = const ApiException(
        'APPLE_MUSIC_NOT_RUNNING',
        'x',
        409,
      );
      expect(await _controller(c).play(result), PlayResult.notRunning);

      client.appPlayError = const AgentUnreachableException();
      expect(await _controller(c).play(result), PlayResult.failed);
      expect(_controller(c).lastPlayFailure, isNotNull);
    });

    test('404 (the search no longer finds it) searches again', () async {
      final client = FakeApiClient();
      final c = await searched(client);
      final searchesBefore = client.appSearchCalls.length;

      client.appPlayError = const ApiException('NOT_FOUND', 'moved', 404);
      final outcome = await _controller(c).play(_state(c).results.first);
      await pumpEventQueue();

      expect(outcome, PlayResult.notFound);
      expect(client.appSearchCalls.length, searchesBefore + 1);
    });
  });
}
