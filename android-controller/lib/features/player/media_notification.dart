import 'package:audio_service/audio_service.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/player_state.dart';
import '../connect/session_controller.dart';
import 'live_controller.dart';
import 'progress.dart';

/// What the notification buttons do. Wired to the API by [mediaNotificationProvider].
abstract interface class MediaCommands {
  Future<void> play();

  Future<void> pause();

  Future<void> next();

  Future<void> previous();

  Future<void> seek(int positionMs);
}

/// Android media session for the media notification and the lock screen.
///
/// The phone plays no audio: this only mirrors the laptop's [PlayerState]
/// and forwards the button presses to the agent. The notification buttons
/// never change what is shown; the next `state` push does.
class AmRemoteAudioHandler extends BaseAudioHandler with SeekHandler {
  /// Set while the app is paired.
  MediaCommands? commands;

  void showPlayer(PlayerState? player, {DateTime? now}) {
    final item = buildMediaItem(player);
    if (item == null) {
      clear();
      return;
    }
    mediaItem.add(item);
    playbackState.add(buildPlaybackState(player!, now ?? DateTime.now()));
  }

  /// Not paired, offline or nothing is playing: take the notification down
  /// and forget the track, so no stale title is left in the media session.
  void clear() {
    mediaItem.add(null);
    playbackState.add(
      PlaybackState(processingState: AudioProcessingState.idle),
    );
  }

  /// The app was swiped away: its buttons can't work any more, so don't leave
  /// a notification behind.
  @override
  Future<void> onTaskRemoved() => stop();

  @override
  Future<void> play() async => _run((c) => c.play());

  @override
  Future<void> pause() async => _run((c) => c.pause());

  @override
  Future<void> skipToNext() async => _run((c) => c.next());

  @override
  Future<void> skipToPrevious() async => _run((c) => c.previous());

  @override
  Future<void> seek(Duration position) async =>
      _run((c) => c.seek(position.inMilliseconds));

  @override
  Future<void> stop() async {
    clear();
    await super.stop();
  }

  Future<void> _run(Future<void> Function(MediaCommands c) action) async {
    final c = commands;
    if (c == null) return;
    try {
      await action(c);
    } catch (_) {
      // Offline or refused: the notification keeps showing the last push.
    }
  }
}

/// The track for the notification, or null when there is nothing to show.
MediaItem? buildMediaItem(PlayerState? player) {
  if (player == null || player.title.isEmpty) return null;
  return MediaItem(
    id: 'am-remote-now-playing',
    title: player.title,
    artist: player.artist.isEmpty ? null : player.artist,
    album: player.album.isEmpty ? null : player.album,
    duration: player.durationMs > 0
        ? Duration(milliseconds: player.durationMs)
        : null,
  );
}

/// Buttons, position and state for one pushed [PlayerState].
PlaybackState buildPlaybackState(PlayerState player, DateTime now) {
  final playing = player.status == PlaybackStatus.playing;
  final active =
      player.status == PlaybackStatus.playing ||
      player.status == PlaybackStatus.paused;
  return PlaybackState(
    controls: [
      MediaControl.skipToPrevious,
      playing ? MediaControl.pause : MediaControl.play,
      MediaControl.skipToNext,
    ],
    systemActions: const {MediaAction.seek},
    androidCompactActionIndices: const [0, 1, 2],
    processingState: active
        ? AudioProcessingState.ready
        : AudioProcessingState.idle,
    playing: playing,
    updatePosition: Duration(milliseconds: player.positionMs),
    updateTime: now,
    speed: playing ? 1.0 : 0.0,
  );
}

/// The handler created at start-up. Null in tests, so nothing touches the
/// platform there.
final audioHandlerProvider = Provider<AmRemoteAudioHandler?>((ref) => null);

/// Keeps the notification in step with the live state while paired.
/// Watch it from a widget that lives as long as the paired session.
final mediaNotificationProvider = Provider<void>((ref) {
  final handler = ref.watch(audioHandlerProvider);
  if (handler == null) return;

  final paired = ref.watch(
    sessionControllerProvider.select((s) => s is SessionPaired),
  );
  if (!paired) {
    handler
      ..commands = null
      ..clear();
    return;
  }

  handler.commands = _ApiMediaCommands(ref);
  ref.onDispose(() => handler.commands = null);

  // What the laptop is doing now. With no connection the buttons can't work
  // and the track may be stale, so the notification goes away until it is back.
  void refresh() {
    final session = ref.read(sessionControllerProvider);
    final live = ref.read(liveControllerProvider);
    final offline =
        session is SessionPaired &&
        session.connection == ConnectionStatus.offline;
    if (offline || live.appleMusicRunning == false) {
      handler.clear();
    } else {
      handler.showPlayer(live.player);
    }
  }

  refresh();
  ref.listen(
    liveControllerProvider.select((s) => (s.player, s.appleMusicRunning)),
    (_, _) => refresh(),
  );
  ref.listen(
    sessionControllerProvider.select(
      (s) => s is SessionPaired && s.connection == ConnectionStatus.offline,
    ),
    (_, _) => refresh(),
  );
});

class _ApiMediaCommands implements MediaCommands {
  _ApiMediaCommands(this._ref);

  final Ref _ref;

  Future<void> _api(Future<void> Function(ApiClient api) call) async {
    final api = _ref.read(apiClientProvider);
    if (api != null) await call(api);
  }

  @override
  Future<void> play() => _api((api) => api.play());

  @override
  Future<void> pause() => _api((api) => api.pause());

  @override
  Future<void> next() => _api((api) => api.next());

  @override
  Future<void> previous() => _api((api) => api.previous());

  @override
  Future<void> seek(int positionMs) => _api((api) {
    final duration = _ref.read(liveControllerProvider).player?.durationMs ?? 0;
    return api.seek(clampSeekMs(positionMs, duration));
  });
}
