import 'package:freezed_annotation/freezed_annotation.dart';

part 'queue_item.freezed.dart';
part 'queue_item.g.dart';

/// One row of `GET /queue` (about the next 20 songs; the current song is in
/// `PlayerState`, not here). There is no stable id: a row is identified by
/// [index] plus [title].
@freezed
abstract class QueueItem with _$QueueItem {
  const QueueItem._();

  const factory QueueItem({
    required int index,
    required String title,
    @Default('') String artist,
    @Default('') String album,

    /// 0 if Apple Music shows none.
    @Default(0) int durationMs,

    /// `queue` (Playing Next) or `autoplay`. Anything else is treated as a
    /// plain queue row.
    @Default('queue') String source,
  }) = _QueueItem;

  factory QueueItem.fromJson(Map<String, Object?> json) =>
      _$QueueItemFromJson(json);

  bool get isAutoplay => source == 'autoplay';
}

/// `GET /queue` answer.
@freezed
abstract class QueueResponse with _$QueueResponse {
  const factory QueueResponse({@Default([]) List<QueueItem> items}) =
      _QueueResponse;

  factory QueueResponse.fromJson(Map<String, Object?> json) =>
      _$QueueResponseFromJson(json);
}

/// `POST /queue/play` body.
@freezed
abstract class QueuePlayRequest with _$QueuePlayRequest {
  const factory QueuePlayRequest({required int index, required String title}) =
      _QueuePlayRequest;

  factory QueuePlayRequest.fromJson(Map<String, Object?> json) =>
      _$QueuePlayRequestFromJson(json);
}
