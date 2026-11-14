import 'package:freezed_annotation/freezed_annotation.dart';

part 'player_state.freezed.dart';
part 'player_state.g.dart';

/// `status` of [PlayerState]. Any value the app doesn't know maps to [unknown].
enum PlaybackStatus { playing, paused, stopped, unknown }

/// `GET /player/state` response and the WebSocket `state` data.
///
/// Missing or null text and number fields become `''` / `0`, so an idle player
/// (the contract doesn't say which one the agent sends) never breaks parsing.
@freezed
abstract class PlayerState with _$PlayerState {
  const factory PlayerState({
    @JsonKey(unknownEnumValue: PlaybackStatus.unknown)
    @Default(PlaybackStatus.unknown)
    PlaybackStatus status,
    @Default('') String title,
    @Default('') String artist,
    @Default('') String album,
    @Default(0) int durationMs,
    @Default(0) int positionMs,

    /// Increments when the artwork changes: fetch `/player/artwork` again.
    @Default(0) int artworkRev,

    /// Only filled if the agent's playlist spike succeeded.
    String? playlistName,
  }) = _PlayerState;

  factory PlayerState.fromJson(Map<String, Object?> json) =>
      _$PlayerStateFromJson(json);
}

/// `POST /player/seek` body.
@freezed
abstract class SeekRequest with _$SeekRequest {
  const factory SeekRequest({required int positionMs}) = _SeekRequest;

  factory SeekRequest.fromJson(Map<String, Object?> json) =>
      _$SeekRequestFromJson(json);
}
