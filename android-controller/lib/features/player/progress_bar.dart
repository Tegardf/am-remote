import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/models/player_state.dart';
import 'live_controller.dart';
import 'player_controller.dart';
import 'progress.dart';

/// Progress bar that moves on its own while playing (every 250 ms) and
/// re-syncs on every `state` push. Dragging seeks when the finger lifts.
class ProgressBar extends ConsumerStatefulWidget {
  const ProgressBar({super.key});

  static const tick = Duration(milliseconds: 250);

  /// How long a position we just sent is shown if no `state` push confirms it.
  static const seekHold = Duration(seconds: 2);

  /// A `state` push confirms a seek when its position is within this many ms
  /// of the target.
  static const confirmTolerance = 3000;

  @override
  ConsumerState<ProgressBar> createState() => _ProgressBarState();
}

class _ProgressBarState extends ConsumerState<ProgressBar> {
  Timer? _ticker;
  Timer? _holdTimer;

  /// Position under the finger while dragging, then until the agent confirms.
  int? _shownMs;
  bool _dragging = false;
  DateTime? _anchor;

  @override
  void dispose() {
    _ticker?.cancel();
    _holdTimer?.cancel();
    super.dispose();
  }

  void _syncTicker(bool playing) {
    if (playing && _ticker == null) {
      _ticker = Timer.periodic(ProgressBar.tick, (_) {
        if (mounted) setState(() {});
      });
    } else if (!playing && _ticker != null) {
      _ticker!.cancel();
      _ticker = null;
    }
  }

  void _release(int ms, DateTime? receivedAt) {
    ref.read(playerControllerProvider.notifier).seek(ms);
    _dragging = false;
    _anchor = receivedAt;
    // Keep showing the target until the next `state` push (or the hold ends).
    _holdTimer?.cancel();
    _holdTimer = Timer(ProgressBar.seekHold, () {
      // A new drag may have started meanwhile: don't pull the thumb away.
      if (mounted && !_dragging) setState(() => _shownMs = null);
    });
  }

  @override
  Widget build(BuildContext context) {
    final live = ref.watch(liveControllerProvider);
    final player = live.player;
    final receivedAt = live.playerReceivedAt;
    final playing = player?.status == PlaybackStatus.playing;
    _syncTicker(playing);

    // The agent answered the seek: a newer `state` than the one at release,
    // whose position is near the target. A push that was already on its way
    // carries the old position and must not snap the bar back.
    if (!_dragging &&
        _shownMs != null &&
        player != null &&
        receivedAt != _anchor &&
        (player.positionMs - _shownMs!).abs() <= ProgressBar.confirmTolerance) {
      _shownMs = null;
    }

    final duration = player?.durationMs ?? 0;
    final now = DateTime.now();
    final current =
        _shownMs ??
        (player == null || receivedAt == null
            ? 0
            : interpolatePositionMs(
                positionMs: player.positionMs,
                durationMs: duration,
                status: player.status,
                receivedAt: receivedAt,
                now: now,
              ));
    final max = duration > 0 ? duration.toDouble() : 1.0;
    final theme = Theme.of(context);

    return Column(
      children: [
        Slider(
          value: current.clamp(0, max.toInt()).toDouble(),
          min: 0,
          max: max,
          onChangeStart: duration > 0
              ? (v) => setState(() {
                  _holdTimer?.cancel();
                  _dragging = true;
                  _shownMs = v.round();
                })
              : null,
          onChanged: duration > 0
              ? (v) => setState(() => _shownMs = v.round())
              : null,
          onChangeEnd: duration > 0
              ? (v) => setState(() {
                  _shownMs = v.round();
                  _release(v.round(), receivedAt);
                })
              : null,
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(formatPosition(current), style: theme.textTheme.bodySmall),
              Text(formatPosition(duration), style: theme.textTheme.bodySmall),
            ],
          ),
        ),
      ],
    );
  }
}
