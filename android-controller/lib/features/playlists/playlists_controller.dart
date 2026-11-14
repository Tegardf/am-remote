import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/playlist.dart';
import '../../core/ws_client.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';
import '../player/live_controller.dart';

enum PlaylistsStatus {
  loading,
  ready,

  /// The agent answered `NOT_SUPPORTED`: it can't read playlists here.
  notSupported,

  /// The agent answered `APPLE_MUSIC_NOT_RUNNING`.
  notRunning,

  /// Anything else: offline, an agent error, a malformed answer.
  error,
}

class PlaylistsState {
  const PlaylistsState({
    this.status = PlaylistsStatus.loading,
    this.items = const [],
    this.playingId,
    this.refreshErrorId = 0,
    this.errorDetail,
  });

  final PlaylistsStatus status;
  final List<Playlist> items;

  /// Why the list could not be read (shown with [PlaylistsStatus.error]).
  final String? errorDetail;

  /// The playlist whose "play" request is in flight.
  final String? playingId;

  /// Counts failed refreshes of a list that is already on screen: the list
  /// stays, and the screen tells the user once per failure.
  final int refreshErrorId;

  PlaylistsState copyWith({
    PlaylistsStatus? status,
    List<Playlist>? items,
    String? playingId,
    bool clearPlaying = false,
    int? refreshErrorId,
    String? errorDetail,
  }) => PlaylistsState(
    status: status ?? this.status,
    items: items ?? this.items,
    playingId: clearPlaying ? null : (playingId ?? this.playingId),
    refreshErrorId: refreshErrorId ?? this.refreshErrorId,
    errorDetail: errorDetail,
  );
}

final playlistsControllerProvider =
    NotifierProvider<PlaylistsController, PlaylistsState>(
      PlaylistsController.new,
    );

class PlaylistsController extends Notifier<PlaylistsState> {
  /// Counts loads: only the latest one may change the screen.
  int _loadId = 0;

  /// The reason the last [play] failed, for the snackbar.
  String? lastPlayFailure;

  @override
  PlaylistsState build() {
    // A new client means a new pairing (or another laptop): the old list is
    // not this laptop's. Start fresh and read it again.
    ref.watch(apiClientProvider);
    // Apple Music was started: a "not running" list can be read now.
    ref.listen(liveControllerProvider.select((s) => s.appleMusicRunning), (
      previous,
      next,
    ) {
      if (next == true && state.status == PlaylistsStatus.notRunning) load();
    });
    // The socket reconnected: ids are not verified across an Apple Music
    // restart, so read the list again (contract C-002). The first connection
    // is not a reconnect: the list is being read for the first time anyway.
    var wasConnected =
        ref.read(liveControllerProvider).link == WsLinkStatus.connected;
    ref.listen(liveControllerProvider.select((s) => s.link), (previous, next) {
      if (next != WsLinkStatus.connected) return;
      if (wasConnected && previous != WsLinkStatus.connected) load();
      wasConnected = true;
    });
    // The connection came back after a failed read: try again by itself.
    ref.listen(
      sessionControllerProvider.select(
        (s) => s is SessionPaired && s.connection == ConnectionStatus.connected,
      ),
      (previous, next) {
        if (next && previous != true && state.status == PlaylistsStatus.error) {
          load();
        }
      },
    );
    Future.microtask(load);
    return const PlaylistsState();
  }

  /// Reads the playlists. Pull-to-refresh awaits it. Keeps the list that is
  /// already there while it runs, and if the refresh fails, so a bad moment
  /// doesn't blank the screen.
  Future<void> load() async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    final id = ++_loadId;
    if (state.items.isEmpty) {
      state = state.copyWith(status: PlaylistsStatus.loading);
    }
    try {
      final items = await api.getPlaylists();
      if (!ref.mounted || id != _loadId) return;
      state = state.copyWith(status: PlaylistsStatus.ready, items: items);
    } catch (e) {
      if (!ref.mounted || id != _loadId) return;
      state = state.items.isEmpty
          ? state.copyWith(status: _statusFor(e), errorDetail: failureReason(e))
          : state.copyWith(refreshErrorId: state.refreshErrorId + 1);
    }
  }

  static PlaylistsStatus _statusFor(Object error) {
    if (error is ApiException) {
      switch (error.code) {
        case ApiErrorCode.notSupported:
          return PlaylistsStatus.notSupported;
        case ApiErrorCode.appleMusicNotRunning:
          return PlaylistsStatus.notRunning;
      }
    }
    return PlaylistsStatus.error;
  }

  /// `POST /play/playlist`. The screen switches to the Player tab on success.
  Future<PlayResult> play(String id) async {
    final api = ref.read(apiClientProvider);
    if (api == null || state.playingId != null) return PlayResult.failed;
    state = state.copyWith(playingId: id);
    try {
      await api.playPlaylist(id);
      return PlayResult.started;
    } catch (e) {
      lastPlayFailure = failureReason(e);
      final result = playResultFor(e);
      // The playlist is gone (deleted on the laptop): the list is out of date.
      if (result == PlayResult.notFound) unawaited(load());
      return result;
    } finally {
      if (ref.mounted) state = state.copyWith(clearPlaying: true);
    }
  }
}
