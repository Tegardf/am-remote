import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/search_result.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';

enum SearchStatus {
  /// Nothing typed yet, or fewer than [SearchController.minQueryLength] characters.
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
    this.status = SearchStatus.idle,
    this.results = const [],
    this.playingUrl,
  });

  /// What is in the text field, as typed.
  final String query;
  final SearchStatus status;
  final List<SearchResult> results;

  /// The result whose "play" request is in flight.
  final String? playingUrl;

  SearchState copyWith({
    String? query,
    SearchStatus? status,
    List<SearchResult>? results,
    String? playingUrl,
    bool clearPlaying = false,
  }) => SearchState(
    query: query ?? this.query,
    status: status ?? this.status,
    results: results ?? this.results,
    playingUrl: clearPlaying ? null : (playingUrl ?? this.playingUrl),
  );
}

final searchControllerProvider =
    NotifierProvider<SearchResultsController, SearchState>(
      SearchResultsController.new,
    );

class SearchResultsController extends Notifier<SearchState> {
  static const debounce = Duration(milliseconds: 300);
  static const minQueryLength = 2;

  /// The agent answers `400` for a longer `q`.
  static const maxQueryLength = 200;
  static const limit = 25;

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
    if (query.length < minQueryLength) {
      // Keep `playingUrl`: a play that is on its way is still on its way.
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

  /// The keyboard's search button: don't wait for the pause.
  void submit() {
    _timer?.cancel();
    final query = queryToSend(state.query);
    if (query.length >= minQueryLength) _search(query);
  }

  /// "Retry" after an error.
  void retry() => submit();

  Future<void> _search(String query) async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    final id = ++_searchId;
    state = state.copyWith(status: SearchStatus.loading);
    try {
      final results = await api.search(query, limit: limit);
      if (!ref.mounted || id != _searchId) return;
      state = state.copyWith(status: SearchStatus.ready, results: results);
    } catch (e) {
      if (!ref.mounted || id != _searchId) return;
      state = state.copyWith(status: _statusFor(e));
    }
  }

  static SearchStatus _statusFor(Object error) {
    if (error is ApiException) {
      switch (error.code) {
        case ApiErrorCode.notSupported:
          return SearchStatus.notSupported;
        case ApiErrorCode.appleMusicNotRunning:
          return SearchStatus.notRunning;
      }
    }
    return SearchStatus.error;
  }

  /// `POST /play/track` with the result's `url`.
  Future<PlayResult> play(SearchResult result) async {
    final api = ref.read(apiClientProvider);
    if (api == null || state.playingUrl != null) return PlayResult.failed;
    state = state.copyWith(playingUrl: result.url);
    try {
      await api.playTrack(result.url);
      return PlayResult.started;
    } catch (e) {
      return playResultFor(e);
    } finally {
      if (ref.mounted) state = state.copyWith(clearPlaying: true);
    }
  }
}
