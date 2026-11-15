import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/queue_item.dart';
import '../../core/ws_client.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';
import 'live_controller.dart';
import 'modes_controller.dart';

enum QueueStatus {
  loading,
  ready,

  /// `501`, or an agent that doesn't know the route: the queue can't be read.
  notSupported,

  /// `409`: Apple Music is closed.
  notRunning,

  /// Anything else: offline, an agent error, a malformed answer.
  error,
}

class QueueState {
  const QueueState({
    this.status = QueueStatus.loading,
    this.items = const [],
    this.playingIndex,
  });

  final QueueStatus status;

  /// About the next 20 songs. The current song is not in here.
  final List<QueueItem> items;

  /// The row whose "play" request is in flight.
  final int? playingIndex;

  QueueState copyWith({
    QueueStatus? status,
    List<QueueItem>? items,
    int? playingIndex,
    bool clearPlaying = false,
  }) => QueueState(
    status: status ?? this.status,
    items: items ?? this.items,
    playingIndex: clearPlaying ? null : (playingIndex ?? this.playingIndex),
  );
}

final queueControllerProvider = NotifierProvider<QueueController, QueueState>(
  QueueController.new,
);

/// The Up Next list. It is read when the Player screen opens, when the track
/// changes, after a shuffle/repeat change, after a reconnect and on
/// pull-to-refresh. Never on a timer.
class QueueController extends Notifier<QueueState> {
  /// Counts reads: only the latest one may change the screen.
  int _loadId = 0;

  /// Why the last [play] failed, for the snackbar.
  String? lastPlayFailure;

  @override
  QueueState build() {
    // A new client means a new pairing: don't keep another laptop's queue.
    ref.watch(apiClientProvider);

    // The track changed (title or cover): the list moved on.
    ref.listen(
      liveControllerProvider.select(
        (s) =>
            s.player == null ? null : (s.player!.title, s.player!.artworkRev),
      ),
      (previous, next) {
        if (previous != null && next != null && next != previous) load();
      },
    );

    // Shuffle reorders the queue; repeat can change what comes next.
    ref.listen(modesControllerProvider.select((s) => s.modes), (
      previous,
      next,
    ) {
      if (previous != null && next != null && next != previous) load();
    });

    // Apple Music was started after a "not running" read.
    ref.listen(liveControllerProvider.select((s) => s.appleMusicRunning), (
      previous,
      next,
    ) {
      if (next == true &&
          previous != true &&
          state.status != QueueStatus.ready) {
        load();
      }
    });

    // The socket reconnected: anything may have happened meanwhile.
    var wasConnected =
        ref.read(liveControllerProvider).link == WsLinkStatus.connected;
    ref.listen(liveControllerProvider.select((s) => s.link), (previous, next) {
      if (next != WsLinkStatus.connected) return;
      if (wasConnected && previous != WsLinkStatus.connected) load();
      wasConnected = true;
    });

    Future.microtask(load);
    return const QueueState();
  }

  /// `GET /queue`. Pull-to-refresh awaits it. A failed refresh of a list that
  /// is already shown keeps the list quietly (a bad moment must not blank it
  /// or spam messages); only "not supported" and "not running" replace it.
  Future<void> load() async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    final id = ++_loadId;
    if (state.items.isEmpty && state.status != QueueStatus.ready) {
      state = state.copyWith(status: QueueStatus.loading);
    }
    try {
      final items = await api.getQueue();
      if (!ref.mounted || id != _loadId) return;
      state = state.copyWith(status: QueueStatus.ready, items: items);
    } catch (e) {
      if (!ref.mounted || id != _loadId) return;
      final status = _statusFor(e);
      if (status == QueueStatus.error && state.items.isNotEmpty) return;
      state = state.copyWith(status: status);
    }
  }

  static QueueStatus _statusFor(Object error) {
    if (error is ApiException) {
      switch (error.code) {
        case ApiErrorCode.notSupported || ApiErrorCode.notFound:
          return QueueStatus.notSupported;
        case ApiErrorCode.appleMusicNotRunning:
          return QueueStatus.notRunning;
      }
    }
    return QueueStatus.error;
  }

  /// `POST /queue/play` for [item]. Takes 1–2 s. A `404` means the queue moved
  /// on (the row is gone or its title changed): the list is read again.
  Future<PlayResult> play(QueueItem item) async {
    final api = ref.read(apiClientProvider);
    if (api == null || state.playingIndex != null) return PlayResult.failed;
    state = state.copyWith(playingIndex: item.index);
    try {
      await api.playQueueItem(item.index, item.title);
      return PlayResult.started;
    } catch (e) {
      lastPlayFailure = failureReason(e);
      final result = playResultFor(e);
      if (result == PlayResult.notFound) unawaited(load());
      return result;
    } finally {
      if (ref.mounted) state = state.copyWith(clearPlaying: true);
    }
  }
}
