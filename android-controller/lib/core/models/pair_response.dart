import 'package:freezed_annotation/freezed_annotation.dart';

part 'pair_response.freezed.dart';
part 'pair_response.g.dart';

/// `POST /pair` response. The token is a secret: never log it.
@freezed
abstract class PairResponse with _$PairResponse {
  const factory PairResponse({required String token}) = _PairResponse;

  factory PairResponse.fromJson(Map<String, Object?> json) =>
      _$PairResponseFromJson(json);
}
