import '../../core/models/player_state.dart';

/// The position to show now, interpolated from the last `state` event.
///
/// While playing, the agent only pushes on change, so the bar advances by the
/// time since [receivedAt]. Otherwise the pushed position is shown as is. The
/// result never goes below 0, and never past [durationMs] when it is known.
int interpolatePositionMs({
  required int positionMs,
  required int durationMs,
  required PlaybackStatus status,
  required DateTime receivedAt,
  required DateTime now,
}) {
  var position = positionMs;
  if (status == PlaybackStatus.playing) {
    final elapsed = now.difference(receivedAt).inMilliseconds;
    if (elapsed > 0) position += elapsed;
  }
  if (durationMs > 0 && position > durationMs) position = durationMs;
  return position < 0 ? 0 : position;
}

/// A seek target the agent accepts: `POST /player/seek` answers `400` outside
/// 0..`duration_ms`. With an unknown duration only the lower bound applies.
int clampSeekMs(int positionMs, int durationMs) {
  if (positionMs < 0) return 0;
  if (durationMs > 0 && positionMs > durationMs) return durationMs;
  return positionMs;
}

/// `m:ss`, or `h:mm:ss` from one hour on.
String formatPosition(int ms) {
  final totalSeconds = (ms < 0 ? 0 : ms) ~/ 1000;
  final hours = totalSeconds ~/ 3600;
  final minutes = (totalSeconds % 3600) ~/ 60;
  final seconds = totalSeconds % 60;
  final ss = seconds.toString().padLeft(2, '0');
  if (hours > 0) return '$hours:${minutes.toString().padLeft(2, '0')}:$ss';
  return '$minutes:$ss';
}
