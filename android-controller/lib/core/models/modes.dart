import 'package:freezed_annotation/freezed_annotation.dart';

part 'modes.freezed.dart';
part 'modes.g.dart';

/// `repeat` of [Modes]: off, the whole queue, or the current song.
enum RepeatSetting {
  off,
  all,
  one;

  /// What one press of the repeat button asks for: off → all → one → off.
  RepeatSetting get next => switch (this) {
    RepeatSetting.off => RepeatSetting.all,
    RepeatSetting.all => RepeatSetting.one,
    RepeatSetting.one => RepeatSetting.off,
  };
}

/// `GET` / `POST /player/modes` answer, and the WebSocket `modes` data.
/// An unknown `repeat` string counts as `off`.
@freezed
abstract class Modes with _$Modes {
  const factory Modes({
    @Default(false) bool shuffle,
    @JsonKey(unknownEnumValue: RepeatSetting.off)
    @Default(RepeatSetting.off)
    RepeatSetting repeat,
  }) = _Modes;

  factory Modes.fromJson(Map<String, Object?> json) => _$ModesFromJson(json);
}

/// `POST /player/modes` body. Either field may be left out; null fields are
/// not sent.
@freezed
abstract class ModesRequest with _$ModesRequest {
  // Freezed copies these options onto the generated class.
  // ignore: invalid_annotation_target
  @JsonSerializable(includeIfNull: false)
  const factory ModesRequest({bool? shuffle, RepeatSetting? repeat}) =
      _ModesRequest;

  factory ModesRequest.fromJson(Map<String, Object?> json) =>
      _$ModesRequestFromJson(json);
}
