import 'package:freezed_annotation/freezed_annotation.dart';

part 'api_error_body.freezed.dart';
part 'api_error_body.g.dart';

/// Non-2xx body: `{ "error": { "code": "…", "message": "…" } }`.
@freezed
abstract class ApiErrorBody with _$ApiErrorBody {
  const factory ApiErrorBody({required ApiErrorDetail error}) = _ApiErrorBody;

  factory ApiErrorBody.fromJson(Map<String, Object?> json) =>
      _$ApiErrorBodyFromJson(json);
}

@freezed
abstract class ApiErrorDetail with _$ApiErrorDetail {
  const factory ApiErrorDetail({
    required String code,
    required String message,
  }) = _ApiErrorDetail;

  factory ApiErrorDetail.fromJson(Map<String, Object?> json) =>
      _$ApiErrorDetailFromJson(json);
}
