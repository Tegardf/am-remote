import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/app_search.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';

enum SearchStatus {
  /// Nothing typed yet, or fewer than [SearchResultsController.minQueryLength]
  /// characters.
  idle,
  loading,
  ready,
  notSupported,
  notRunning,
  error,
}

class SearchState {
  const SearchState({
    this.query = '',
    this.scope = SearchScope.catalog,
    this.status = SearchStatus.idle,
    this.results = const [],
    this.searchedQuery = '',
    this.searchedScope = SearchScope.catalog,
    this.playingKey,
  });

  /// What is in the text field, as typed.
  final String query;

  /// The Catalog / Library switch.
  final SearchScope scope;
  final SearchStatus status;
  final List<AppSearchResult> results;

  /// The query and scope that produced [results]. A result is played with
  /// these, not with what is in the box now: the agent repeats that search and
  /// finds the result again by section, index and title.
  final String searchedQuery;
  final SearchScope searchedScope;

  /// The result whose "play" request is in flight ([resultKey]).
  final String? playingKey;

  SearchState copyWith({
    String? query,
    SearchScope? scope,
    SearchStatus? status,
    List<AppSearchResult>? results,
    String? searchedQuery,
    SearchScope? searchedScope,
    String? playingKey,
    bool clearPlaying = false,
  }) => SearchState(
    query: query ?? this.query,
    scope: scope ?? this.scope,
    status: status ?? this.status,
    results: results ?? this.results,
    searchedQuery: searchedQuery ?? this.searchedQuery,
    searchedScope: searchedScope ?? this.searchedScope,
    playingKey: clearPlaying ? null : (playingKey ?? this.playingKey),
  );

  /// The results grouped by `section`, in the order the agent sent them
  /// (a section is placed where it first appears).
  List<({String section, List<AppSearchResult> results})> get sections {
    final grouped = <String, List<AppSearchResult>>{};
    for (final result in results) {
      grouped.putIfAbsent(result.section, () => []).add(result);
    }
    return [
      for (final entry in grouped.entries)
        (section: entry.key, results: entry.value),
    ];
  }
}

/// Identifies a result within one answer (there is no id).
String resultKey(AppSearchResult result) =>
    '${result.section}\u0000${result.index}\u0000${result.title}';

final searchControllerProvider =
    NotifierProvider<SearchResultsController, SearchState>(
      SearchResultsController.new,
    );

/// Search through the Apple Music app's own search (`/app-search`). A search
/// takes about 4 s, and it moves the app's window to its search page.
class SearchResultsController extends Notifier<SearchState> {
  static const debounce = Duration(milliseconds: 300);
  static const minQueryLength = 2;

  /// The agent answers `400` for a longer `q`.
  static const maxQueryLength = 200;

  /// What is sent: the text without the spaces around it, at most
  /// [maxQueryLength] characters.
  static String queryToSend(String text) {
    final trimmed = text.trim();
    final characters = trimmed.runes;
    return characters.length <= maxQueryLength
        ? trimmed
        : String.fromCharCodes(characters.take(maxQueryLength)).trim();
  }

  Timer? _timer;

  /// Counts searches: an answer for an older one is dropped.
  int _searchId = 0;

  /// Why the last [play] failed, for the snackbar.
  String? lastPlayFailure;

  @override
  SearchState build() {
    // A new client means a new pairing (or another laptop): start fresh, and
    // don't keep the text, results or error of the one before.
    ref.watch(apiClientProvider);
    ref.onDispose(() => _timer?.cancel());
    return const SearchState();
  }

  /// The text changed. A search starts 300 ms after the last keystroke, and
  /// only once there are at least 2 characters (spaces around don't count).
  void setQuery(String text) {
    _timer?.cancel();
    // Whatever was running is for older text: forget its answer.
    _searchId++;
    final query = queryToSend(text);
    if (query.runes.length < minQueryLength) {
      // Keep `playingKey`: a play that is on its way is still on its way.
      state = state.copyWith(
        query: text,
        status: SearchStatus.idle,
        results: const [],
      );
      return;
    }
    state = state.copyWith(
      query: text,
      // The search that was loading was just forgotten.
      status: state.status == SearchStatus.loading
          ? (state.results.isEmpty ? SearchStatus.idle : SearchStatus.ready)
          : state.status,
    );
    _timer = Timer(debounce, () => _search(query));
  }

  /// Catalog or Library. Searches again at once (no pause: this is a tap).
  void setScope(SearchScope scope) {
    if (scope == state.scope) return;
    _timer?.cancel();
    _searchId++;
    // The shown results belong to the other scope.
    state = state.copyWith(
      scope: scope,
      results: const [],
      status: SearchStatus.idle,
    );
    submit();
  }

  /// The keyboard's search button: don't wait for the pause.
  void submit() {
    _timer?.cancel();
    final query = queryToSend(state.query);
    if (query.runes.length >= minQueryLength) _search(query);
  }

  /// "Retry" after an error.
  void retry() => submit();

  Future<void> _search(String query) async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    final id = ++_searchId;
    final scope = state.scope;
    state = state.copyWith(status: SearchStatus.loading);
    try {
      final results = await api.appSearch(query, scope: scope);
      if (!ref.mounted || id != _searchId) return;
      state = state.copyWith(
        status: SearchStatus.ready,
        results: results,
        searchedQuery: query,
        searchedScope: scope,
      );
    } catch (e) {
      if (!ref.mounted || id != _searchId) return;
      state = state.copyWith(status: _statusFor(e));
    }
  }

  static SearchStatus _statusFor(Object error) {
    if (error is ApiException) {
      switch (error.code) {
        // 501, or an agent that doesn't know the route (404).
        case ApiErrorCode.notSupported || ApiErrorCode.notFound:
          return SearchStatus.notSupported;
        case ApiErrorCode.appleMusicNotRunning:
          return SearchStatus.notRunning;
      }
    }
    return SearchStatus.error;
  }

  /// `POST /app-search/play` with the search that produced [result] and the
  /// result's own `section`, `index` and `title`, unchanged. Takes 1–5 s. A
  /// `404` means the search no longer finds it: the list is read again.
  Future<PlayResult> play(AppSearchResult result) async {
    final api = ref.read(apiClientProvider);
    if (api == null || state.playingKey != null) return PlayResult.failed;
    state = state.copyWith(playingKey: resultKey(result));
    try {
      await api.playAppResult(
        q: state.searchedQuery,
        scope: state.searchedScope,
        section: result.section,
        index: result.index,
        title: result.title,
      );
      return PlayResult.started;
    } catch (e) {
      lastPlayFailure = failureReason(e);
      final outcome = playResultFor(e);
      if (outcome == PlayResult.notFound) submit();
      return outcome;
    } finally {
      if (ref.mounted) state = state.copyWith(clearPlaying: true);
    }
  }
}
