import 'package:freezed_annotation/freezed_annotation.dart';

part 'search_result.freezed.dart';
part 'search_result.g.dart';

/// One entry of `GET /search`.
///
/// A missing or null artist, album or artwork URL becomes `''`, so a sparse
/// result still shows. [artworkUrl] is an internet URL: the app loads it only
/// when it is HTTPS.
@freezed
abstract class SearchResult with _$SearchResult {
  const factory SearchResult({
    required String id,
    required String title,
    @Default('') String artist,
    @Default('') String album,
    @Default('') String artworkUrl,

    /// The `music.apple.com` link that `POST /play/track` takes.
    required String url,
  }) = _SearchResult;

  factory SearchResult.fromJson(Map<String, Object?> json) =>
      _$SearchResultFromJson(json);
}

/// `POST /play/track` body.
@freezed
abstract class PlayTrackRequest with _$PlayTrackRequest {
  const factory PlayTrackRequest({required String url}) = _PlayTrackRequest;

  factory PlayTrackRequest.fromJson(Map<String, Object?> json) =>
      _$PlayTrackRequestFromJson(json);
}
