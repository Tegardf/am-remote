import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/features/player/progress.dart';
import 'package:flutter_test/flutter_test.dart';

final _t0 = DateTime(2026, 11, 14, 12);

int _at(
  PlaybackStatus status,
  int elapsedMs, {
  int position = 42000,
  int duration = 215000,
}) => interpolatePositionMs(
  positionMs: position,
  durationMs: duration,
  status: status,
  receivedAt: _t0,
  now: _t0.add(Duration(milliseconds: elapsedMs)),
);

void main() {
  group('interpolatePositionMs', () {
    test('advances with the time since the last push while playing', () {
      expect(_at(PlaybackStatus.playing, 0), 42000);
      expect(_at(PlaybackStatus.playing, 250), 42250);
      expect(_at(PlaybackStatus.playing, 3000), 45000);
    });

    test('stands still when paused, stopped or unknown', () {
      for (final status in [
        PlaybackStatus.paused,
        PlaybackStatus.stopped,
        PlaybackStatus.unknown,
      ]) {
        expect(_at(status, 5000), 42000, reason: status.name);
      }
    });

    test('never runs past the duration', () {
      expect(_at(PlaybackStatus.playing, 500000), 215000);
      expect(_at(PlaybackStatus.playing, 0, position: 300000), 215000);
    });

    test('an unknown duration (0) does not clamp', () {
      expect(_at(PlaybackStatus.playing, 4000, duration: 0), 46000);
    });

    test('never goes below 0, even if the clock steps back', () {
      expect(_at(PlaybackStatus.playing, -5000), 42000);
      expect(_at(PlaybackStatus.paused, 0, position: -10), 0);
    });
  });

  group('formatPosition', () {
    test('formats minutes and seconds', () {
      expect(formatPosition(0), '0:00');
      expect(formatPosition(999), '0:00');
      expect(formatPosition(1000), '0:01');
      expect(formatPosition(42000), '0:42');
      expect(formatPosition(215000), '3:35');
      expect(formatPosition(600000), '10:00');
    });

    test('adds hours from one hour on, and clamps negatives', () {
      expect(formatPosition(3600000), '1:00:00');
      expect(formatPosition(3725000), '1:02:05');
      expect(formatPosition(-5), '0:00');
    });
  });

  group('clampSeekMs (the agent answers 400 outside 0..duration)', () {
    test('keeps a position inside the track', () {
      expect(clampSeekMs(0, 215000), 0);
      expect(clampSeekMs(90000, 215000), 90000);
      expect(clampSeekMs(215000, 215000), 215000);
    });

    test('pulls a position back into range', () {
      expect(clampSeekMs(-5, 215000), 0);
      expect(clampSeekMs(215001, 215000), 215000);
    });

    test('with an unknown duration only the lower bound applies', () {
      expect(clampSeekMs(90000, 0), 90000);
      expect(clampSeekMs(-1, 0), 0);
    });
  });
}
