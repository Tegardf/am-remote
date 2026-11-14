import 'package:am_remote/core/models/player_state.dart';
import 'package:am_remote/features/player/media_notification.dart';
import 'package:audio_service/audio_service.dart';
import 'package:flutter_test/flutter_test.dart';

final _now = DateTime(2026, 11, 14, 12);

PlayerState _state(
  PlaybackStatus status, {
  String title = 'Blue Monday',
  int position = 42000,
}) => PlayerState(
  status: status,
  title: title,
  artist: 'New Order',
  album: 'Power, Corruption & Lies',
  durationMs: 215000,
  positionMs: position,
);

void main() {
  group('buildPlaybackState', () {
    test('playing shows pause between previous and next, at normal speed', () {
      final s = buildPlaybackState(_state(PlaybackStatus.playing), _now);
      expect(s.playing, isTrue);
      expect(s.controls, [
        MediaControl.skipToPrevious,
        MediaControl.pause,
        MediaControl.skipToNext,
      ]);
      expect(s.processingState, AudioProcessingState.ready);
      expect(s.speed, 1.0);
      expect(s.updatePosition, const Duration(milliseconds: 42000));
      expect(s.updateTime, _now);
      expect(s.systemActions, contains(MediaAction.seek));
    });

    test('paused shows play and stands still', () {
      final s = buildPlaybackState(_state(PlaybackStatus.paused), _now);
      expect(s.playing, isFalse);
      expect(s.controls[1], MediaControl.play);
      expect(s.processingState, AudioProcessingState.ready);
      expect(s.speed, 0.0);
    });

    test('stopped and unknown are idle, so the notification can go away', () {
      for (final status in [PlaybackStatus.stopped, PlaybackStatus.unknown]) {
        final s = buildPlaybackState(_state(status), _now);
        expect(
          s.processingState,
          AudioProcessingState.idle,
          reason: status.name,
        );
        expect(s.playing, isFalse);
      }
    });

    test('the compact view shows all three buttons', () {
      final s = buildPlaybackState(_state(PlaybackStatus.playing), _now);
      expect(s.androidCompactActionIndices, [0, 1, 2]);
    });
  });

  group('buildMediaItem', () {
    test('copies title, artist, album and duration', () {
      final item = buildMediaItem(_state(PlaybackStatus.playing))!;
      expect(item.title, 'Blue Monday');
      expect(item.artist, 'New Order');
      expect(item.album, 'Power, Corruption & Lies');
      expect(item.duration, const Duration(milliseconds: 215000));
    });

    test('leaves out an unknown duration and empty names', () {
      final item = buildMediaItem(
        const PlayerState(
          status: PlaybackStatus.playing,
          title: 'Only a title',
        ),
      )!;
      expect(item.artist, isNull);
      expect(item.album, isNull);
      expect(item.duration, isNull);
    });

    test('is null with no state or an empty title', () {
      expect(buildMediaItem(null), isNull);
      expect(buildMediaItem(_state(PlaybackStatus.stopped, title: '')), isNull);
    });
  });
}
