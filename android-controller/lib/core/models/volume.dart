import 'package:freezed_annotation/freezed_annotation.dart';

part 'volume.freezed.dart';
part 'volume.g.dart';

/// `GET /volume` and `POST /volume` response, and the WebSocket `volume` data.
/// [level] is 0-100.
@freezed
abstract class Volume with _$Volume {
  const factory Volume({required int level, required bool muted}) = _Volume;

  factory Volume.fromJson(Map<String, Object?> json) => _$VolumeFromJson(json);
}

/// `POST /volume` body. Either field may be left out; null fields are not sent.
@freezed
abstract class VolumeRequest with _$VolumeRequest {
  // Freezed copies these options onto the generated class.
  // ignore: invalid_annotation_target
  @JsonSerializable(includeIfNull: false)
  const factory VolumeRequest({int? level, bool? muted}) = _VolumeRequest;

  factory VolumeRequest.fromJson(Map<String, Object?> json) =>
      _$VolumeRequestFromJson(json);
}
