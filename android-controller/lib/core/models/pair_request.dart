import 'package:freezed_annotation/freezed_annotation.dart';

part 'pair_request.freezed.dart';
part 'pair_request.g.dart';

/// `POST /pair` body.
@freezed
abstract class PairRequest with _$PairRequest {
  const factory PairRequest({
    required String code,
    required String deviceName,
  }) = _PairRequest;

  factory PairRequest.fromJson(Map<String, Object?> json) =>
      _$PairRequestFromJson(json);
}
