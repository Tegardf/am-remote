import 'dart:async';

import 'package:am_remote/core/api_client.dart';
import 'package:am_remote/core/models/search_result.dart';
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

SearchResult _result(String title) => SearchResult(
  id: title,
  title: title,
  url: 'https://music.apple.com/$title',
);

void main() {
  test(
    'a search text is trimmed and cut to 200 characters (longer is a 400)',
    () {
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
        expect(client.searchCalls.single.query, 'b' * 200);
      });
    },
  );

  test('typing "a" then "ab" quickly sends one request, for "ab"', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);

      _controller(c).setQuery('a');
      async.elapse(const Duration(milliseconds: 100));
      _controller(c).setQuery('ab');
      async.elapse(const Duration(milliseconds: 299));
      expect(client.searchCalls, isEmpty, reason: 'still waiting');

      async.elapse(const Duration(milliseconds: 1));
      async.flushMicrotasks();
      expect(client.searchCalls, [(query: 'ab', limit: 25)]);
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
      expect(client.searchCalls, isEmpty);

      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(client.searchCalls.map((s) => s.query), ['abcde']);
    });
  });

  test('one character sends nothing', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);

      _controller(c).setQuery('a');
      async.elapse(const Duration(seconds: 2));
      async.flushMicrotasks();

      expect(client.searchCalls, isEmpty);
      expect(_state(c).status, SearchStatus.idle);
    });
  });

  test('spaces around the text do not count towards the 2 characters', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);

      _controller(c).setQuery('  a ');
      async.elapse(const Duration(seconds: 1));
      expect(client.searchCalls, isEmpty);

      _controller(c).setQuery('  ab ');
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(client.searchCalls.single.query, 'ab', reason: 'trimmed');
    });
  });

  test('clearing the text goes back to idle and drops the results', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);
      _controller(c).setQuery('ab');
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(_state(c).results, isNotEmpty);

      _controller(c).setQuery('');

      expect(_state(c).status, SearchStatus.idle);
      expect(_state(c).results, isEmpty);
    });
  });

  test('deleting back to one character forgets a search that is running', () {
    fakeAsync((async) {
      final client = FakeApiClient()..searchGate = Completer<void>();
      final c = _container(client);
      _controller(c).setQuery('ab');
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(_state(c).status, SearchStatus.loading);

      _controller(c).setQuery('a');
      client.searchGate!.complete();
      async.flushMicrotasks();

      expect(_state(c).status, SearchStatus.idle);
      expect(_state(c).results, isEmpty, reason: 'the late answer is dropped');
    });
  });

  test('a late answer for an older query does not replace the newer one', () {
    fakeAsync((async) {
      final client = FakeApiClient()..searchGate = Completer<void>();
      final c = _container(client);

      client.searchResults = [_result('for ab')];
      _controller(c).setQuery('ab');
      async.elapse(_debounce);
      async.flushMicrotasks();

      client.searchResults = [_result('for abc')];
      _controller(c).setQuery('abc');
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(client.searchCalls.map((s) => s.query), ['ab', 'abc']);

      client.searchGate!.complete();
      async.flushMicrotasks();

      expect(_state(c).results.single.title, 'for abc');
    });
  });

  test(
    'clearing the text while a track is starting keeps it marked as playing',
    () {
      fakeAsync((async) {
        final client = FakeApiClient()..playGate = Completer<void>();
        final c = _container(client);
        _controller(c).setQuery('ab');
        _controller(c).submit();
        async.flushMicrotasks();

        unawaited(_controller(c).play(_result('first')));
        async.flushMicrotasks();
        expect(_state(c).playingUrl, isNotNull);

        _controller(c).setQuery('');
        expect(_state(c).playingUrl, isNotNull, reason: 'still on its way');

        // A second tap meanwhile is ignored.
        unawaited(_controller(c).play(_result('second')));
        async.flushMicrotasks();
        expect(client.playedTracks, hasLength(1));

        client.playGate!.complete();
        async.flushMicrotasks();
        expect(_state(c).playingUrl, isNull);
      });
    },
  );

  test('typing more while a search runs drops the older answer', () {
    fakeAsync((async) {
      final client = FakeApiClient()..searchGate = Completer<void>();
      final c = _container(client);
      client.searchResults = [_result('for ab')];
      _controller(c).setQuery('ab');
      _controller(c).submit();
      async.flushMicrotasks();
      expect(_state(c).status, SearchStatus.loading);

      // Text changes: the running search is for older text, so its spinner
      // stops and its answer will not be shown.
      _controller(c).setQuery('abc');
      expect(_state(c).status, SearchStatus.idle);
      client.searchGate!.complete();
      async.flushMicrotasks();
      expect(_state(c).results, isEmpty);
      expect(_state(c).status, SearchStatus.idle);

      client.searchResults = [_result('for abc')];
      async.elapse(_debounce);
      async.flushMicrotasks();
      expect(_state(c).results.single.title, 'for abc');
    });
  });

  test('submit searches at once, without waiting for the pause', () {
    fakeAsync((async) {
      final client = FakeApiClient();
      final c = _container(client);

      _controller(c).setQuery('blue monday');
      _controller(c).submit();
      async.flushMicrotasks();

      expect(client.searchCalls.single.query, 'blue monday');
      // The pending timer was cancelled: no second request.
      async.elapse(const Duration(seconds: 1));
      async.flushMicrotasks();
      expect(client.searchCalls, hasLength(1));
    });
  });

  test(
    'NOT_SUPPORTED, APPLE_MUSIC_NOT_RUNNING and other errors are told apart',
    () {
      fakeAsync((async) {
        final client = FakeApiClient();
        final c = _container(client);

        for (final (error, status) in [
          (
            const ApiException('NOT_SUPPORTED', 'no', 501),
            SearchStatus.notSupported,
          ),
          (
            const ApiException('APPLE_MUSIC_NOT_RUNNING', 'closed', 409),
            SearchStatus.notRunning,
          ),
          (const ApiException('INTERNAL', 'boom', 500), SearchStatus.error),
          (const AgentUnreachableException(), SearchStatus.error),
          (const FormatException('bad'), SearchStatus.error),
        ]) {
          client.searchError = error;
          _controller(c).setQuery('ab');
          _controller(c).submit();
          async.flushMicrotasks();
          expect(_state(c).status, status, reason: '$error');
        }
      });
    },
  );

  test('retry runs the same search again', () {
    fakeAsync((async) {
      final client = FakeApiClient()
        ..searchError = const AgentUnreachableException();
      final c = _container(client);
      _controller(c).setQuery('ab');
      _controller(c).submit();
      async.flushMicrotasks();
      expect(_state(c).status, SearchStatus.error);

      client.searchError = null;
      _controller(c).retry();
      async.flushMicrotasks();

      expect(client.searchCalls.map((s) => s.query), ['ab', 'ab']);
      expect(_state(c).status, SearchStatus.ready);
    });
  });

  test('playing a result sends its url', () async {
    final client = FakeApiClient();
    final c = _container(client);
    final result = _result('Example');

    final outcome = await _controller(c).play(result);

    expect(outcome, PlayResult.started);
    expect(client.playedTracks, [result.url]);
    expect(_state(c).playingUrl, isNull);
  });

  test('a play that fails reports why', () async {
    final client = FakeApiClient();
    final c = _container(client);

    client.playError = const ApiException('NOT_SUPPORTED', 'no', 501);
    expect(await _controller(c).play(_result('a')), PlayResult.notSupported);

    client.playError = const ApiException('APPLE_MUSIC_NOT_RUNNING', 'x', 409);
    expect(await _controller(c).play(_result('a')), PlayResult.notRunning);

    client.playError = const ApiException('NOT_FOUND', 'unknown song', 404);
    expect(await _controller(c).play(_result('a')), PlayResult.notFound);

    client.playError = const AgentUnreachableException();
    expect(await _controller(c).play(_result('a')), PlayResult.failed);
  });
}
