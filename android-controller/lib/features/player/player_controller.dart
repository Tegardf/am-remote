import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/player_state.dart';
import '../../core/models/volume.dart';
import '../../core/ws_client.dart';
import '../connect/session_controller.dart';
import 'live_controller.dart';
import 'progress.dart';

class PlayerControlsState {
  const PlayerControlsState({
    this.busy = false,
    this.notRunning = false,
    this.volume = 0,
    this.muted = false,
    this.volumeLoaded = false,
    this.volumeUnavailable = false,
    this.dragging = false,
    this.error,
    this.errorId = 0,
  });

  /// A transport or mute request is in flight: its buttons are disabled.
  final bool busy;

  /// The agent answered APPLE_MUSIC_NOT_RUNNING.
  final bool notRunning;

  /// 0-100.
  final int volume;
  final bool muted;
  final bool volumeLoaded;

  /// The agent answered NO_AUDIO_SESSION: Apple Music is there, but Windows has
  /// no audio session for it yet (nothing has played). Only the volume controls
  /// are off; the playback buttons and the now-playing view work.
  final bool volumeUnavailable;

  /// The user is moving the slider: pushed volume values must not move it.
  final bool dragging;

  /// Short message for a snackbar. [errorId] changes on every new error so the
  /// same text can be shown twice.
  final String? error;
  final int errorId;

  PlayerControlsState copyWith({
    bool? busy,
    bool? notRunning,
    int? volume,
    bool? muted,
    bool? volumeLoaded,
    bool? volumeUnavailable,
    bool? dragging,
    String? error,
    int? errorId,
  }) => PlayerControlsState(
    busy: busy ?? this.busy,
    notRunning: notRunning ?? this.notRunning,
    volume: volume ?? this.volume,
    muted: muted ?? this.muted,
    volumeLoaded: volumeLoaded ?? this.volumeLoaded,
    volumeUnavailable: volumeUnavailable ?? this.volumeUnavailable,
    dragging: dragging ?? this.dragging,
    error: error ?? this.error,
    errorId: errorId ?? this.errorId,
  );
}

final playerControllerProvider =
    NotifierProvider.autoDispose<PlayerController, PlayerControlsState>(
      PlayerController.new,
    );

/// Buttons send an HTTP command and wait: nothing is updated optimistically
/// except the volume slider, which follows the finger and sends throttled
/// `POST /volume` calls (the last value is always sent).
class PlayerController extends Notifier<PlayerControlsState> {
  static const volumeSendInterval = Duration(milliseconds: 100);

  Timer? _volumeTimer;
  int? _pendingLevel;
  bool _sendingVolume = false;
  bool _retrying = false;

  /// Bumped whenever the app sends a volume or mute change. A `GET /volume`
  /// that started earlier is stale once this moves, so its answer is dropped.
  int _volumeVersion = 0;

  @override
  PlayerControlsState build() {
    ref.onDispose(() => _volumeTimer?.cancel());
    // Read the volume again whenever the agent becomes usable.
    ref.listen(sessionControllerProvider.select(_usable), (previous, next) {
      // retry() reloads by itself, exactly once.
      if (next && previous != true && !_retrying) loadVolume();
    });
    // `volume` pushes from the laptop move the slider, except while it is
    // being dragged or a change of ours is on its way.
    ref.listen(liveControllerProvider.select((s) => s.volumeSeq), (_, _) {
      final pushed = ref.read(liveControllerProvider).volume;
      if (pushed != null) _applyPushedVolume(pushed);
    });
    // Apple Music was started: its controls are usable again. When it stops,
    // the audio session goes with it and no `volume` event says so, so the last
    // level is stale until it is read again.
    ref.listen(liveControllerProvider.select((s) => s.appleMusicRunning), (
      previous,
      next,
    ) {
      if (next == true && previous != true) {
        state = state.copyWith(notRunning: false);
        loadVolume();
      } else if (next == false && previous != false) {
        state = state.copyWith(volumeLoaded: false);
      }
    });
    // The socket reconnected: the agent sends no `volume` without an audio
    // session, so a level from before the drop may be stale. Read it again
    // (contract C-003). The first connection is not a reconnect.
    var wasConnected =
        ref.read(liveControllerProvider).link == WsLinkStatus.connected;
    ref.listen(liveControllerProvider.select((s) => s.link), (previous, next) {
      if (next != WsLinkStatus.connected) return;
      if (wasConnected && previous != WsLinkStatus.connected) loadVolume();
      wasConnected = true;
    });
    // Nothing has played yet, so there is no audio session: once something
    // plays there may be one. A `volume` push is not promised for that moment,
    // so ask again when the track or status changes.
    ref.listen(
      liveControllerProvider.select(
        (s) => (s.player?.status, s.player?.artworkRev),
      ),
      (previous, next) {
        if (state.volumeUnavailable &&
            next.$1 == PlaybackStatus.playing &&
            next != previous) {
          loadVolume();
        }
      },
    );
    Future.microtask(loadVolume);
    return const PlayerControlsState();
  }

  void _applyPushedVolume(Volume pushed) {
    if (state.dragging || _pendingLevel != null || _sendingVolume) return;
    // Newer than any GET /volume that is still on its way.
    _volumeVersion++;
    state = state.copyWith(
      volume: pushed.level.clamp(0, 100),
      muted: pushed.muted,
      volumeLoaded: true,
      volumeUnavailable: false,
    );
  }

  /// `POST /player/seek`. The new position comes back as a `state` push.
  Future<void> seek(int positionMs) async {
    final api = _api;
    if (api == null) return;
    try {
      final duration = ref.read(liveControllerProvider).player?.durationMs ?? 0;
      await api.seek(clampSeekMs(positionMs, duration));
    } catch (e) {
      _fail(e);
    }
  }

  static bool _usable(SessionState s) =>
      s is SessionPaired &&
      s.connection == ConnectionStatus.connected &&
      s.status?.appleMusicRunning != false;

  ApiClient? get _api => ref.read(apiClientProvider);

  Future<void> play() => _command((api) => api.play());

  Future<void> pause() => _command((api) => api.pause());

  Future<void> toggle() => _command((api) => api.toggle());

  Future<void> next() => _command((api) => api.next());

  Future<void> previous() => _command((api) => api.previous());

  Future<void> _command(Future<void> Function(ApiClient api) run) async {
    final api = _api;
    if (api == null || state.busy) return;
    final wasNotRunning = state.notRunning;
    state = state.copyWith(busy: true);
    try {
      await run(api);
      if (!ref.mounted) return;
      state = state.copyWith(busy: false, notRunning: false);
      // Apple Music is back, and the volume was marked stale when it went away.
      if (wasNotRunning) unawaited(loadVolume());
    } catch (e) {
      _fail(e, clearBusy: true);
    }
  }

  Future<void> loadVolume() async {
    final api = _api;
    if (api == null) return;
    final version = _volumeVersion;
    try {
      final volume = await api.getVolume();
      if (!ref.mounted) return;
      if (state.dragging || _pendingLevel != null) return;
      if (version != _volumeVersion) return;
      state = state.copyWith(
        volume: volume.level.clamp(0, 100),
        muted: volume.muted,
        volumeLoaded: true,
        volumeUnavailable: false,
        notRunning: false,
      );
    } catch (e) {
      _fail(e, quiet: true);
    }
  }

  void volumeDragStart() {
    state = state.copyWith(dragging: true);
  }

  /// The slider moved: show it now, send at most once per interval.
  void volumeChanged(int level) {
    _pendingLevel = level.clamp(0, 100);
    state = state.copyWith(volume: _pendingLevel);
    _volumeTimer ??= Timer(volumeSendInterval, _flushVolume);
  }

  /// The finger lifted: send the final value right away.
  void volumeDragEnd(int level) {
    _pendingLevel = level.clamp(0, 100);
    _volumeTimer?.cancel();
    _volumeTimer = null;
    state = state.copyWith(volume: _pendingLevel, dragging: false);
    _flushVolume();
  }

  Future<void> _flushVolume() async {
    _volumeTimer?.cancel();
    _volumeTimer = null;
    final api = _api;
    final level = _pendingLevel;
    // While a request is in flight the pending value waits; the `finally`
    // below sends it as soon as the answer is back.
    if (api == null || level == null || _sendingVolume) return;
    _pendingLevel = null;
    _sendingVolume = true;
    _volumeVersion++;
    try {
      final volume = await api.setVolume(level: level);
      if (!ref.mounted) return;
      // Keep the slider where the finger is while it is still moving.
      final settled = !state.dragging && _pendingLevel == null;
      state = state.copyWith(
        volume: settled ? volume.level.clamp(0, 100) : null,
        muted: volume.muted,
        volumeUnavailable: false,
        notRunning: false,
      );
    } catch (e) {
      _fail(e);
      loadVolume();
    } finally {
      _sendingVolume = false;
      if (ref.mounted && _pendingLevel != null) _flushVolume();
    }
  }

  Future<void> toggleMute() async {
    final api = _api;
    if (api == null || state.busy || !state.volumeLoaded) return;
    state = state.copyWith(busy: true);
    _volumeVersion++;
    try {
      final volume = await api.setVolume(muted: !state.muted);
      if (!ref.mounted) return;
      state = state.copyWith(
        busy: false,
        volume: volume.level.clamp(0, 100),
        muted: volume.muted,
        volumeUnavailable: false,
        notRunning: false,
      );
    } catch (e) {
      _fail(e, clearBusy: true);
    }
  }

  /// "Retry" on the empty state: ask the agent again. The volume answer
  /// decides whether the empty state stays (409) or the controls come back.
  Future<void> retry() async {
    state = state.copyWith(notRunning: false);
    _retrying = true;
    try {
      await ref
          .read(sessionControllerProvider.notifier)
          .checkConnection(showConnecting: true);
      await loadVolume();
    } finally {
      _retrying = false;
    }
  }

  /// Maps a failed call onto state. A 401 is handled centrally by the client
  /// (token cleared, back to pairing); an unreachable laptop shows in the
  /// connection indicator.
  ///
  /// [clearBusy] is only for the calls that set `busy` (transport and mute): a
  /// failed volume call must not re-enable buttons whose request is running.
  void _fail(Object e, {bool quiet = false, bool clearBusy = false}) {
    if (!ref.mounted) return;
    var next = clearBusy ? state.copyWith(busy: false) : state;
    String? message;
    if (e is ApiException) {
      switch (e.code) {
        case ApiErrorCode.appleMusicNotRunning:
          // No media session at all, so no audio session either: the last
          // volume level is stale, not just the controls.
          next = next.copyWith(notRunning: true, volumeLoaded: false);
        case ApiErrorCode.noAudioSession:
          // Apple Music is there; only the volume is out of reach for now. No
          // empty state and no message: just the volume controls go off, and
          // the last level is stale.
          next = next.copyWith(volumeLoaded: false, volumeUnavailable: true);
        case ApiErrorCode.unauthorized:
          break;
        case ApiErrorCode.notSupported:
          message = 'Not supported on this laptop';
        default:
          message = 'The laptop reported an error';
      }
    } else if (e is AgentUnreachableException) {
      message = "Can't reach the laptop";
    } else {
      message = 'Something went wrong';
    }
    if (message != null && !quiet) {
      next = next.copyWith(error: message, errorId: state.errorId + 1);
    }
    state = next;
  }
}
