import 'package:freezed_annotation/freezed_annotation.dart';

part 'app_search.freezed.dart';
part 'app_search.g.dart';

/// Where `GET /app-search` looks: Apple Music's catalog, or the user's own
/// library. [name] is what goes on the wire.
enum SearchScope { catalog, library }

/// One result of `GET /app-search`. There is no link, id or artwork: a result
/// is found again by [section], [index] and [title].
@freezed
abstract class AppSearchResult with _$AppSearchResult {
  const factory AppSearchResult({
    /// The heading the app shows, for example `Songs` or `Top Results`.
    required String section,

    /// The position inside [section].
    required int index,

    /// song | album | artist | playlist | video | other. Not used to decide
    /// anything: the agent answers 501 for a kind it can't play yet.
    @Default('other') String kind,
    required String title,

    /// Artist or curator, `''` if none.
    @Default('') String subtitle,
  }) = _AppSearchResult;

  factory AppSearchResult.fromJson(Map<String, Object?> json) =>
      _$AppSearchResultFromJson(json);
}

/// `GET /app-search` answer.
@freezed
abstract class AppSearchResponse with _$AppSearchResponse {
  const factory AppSearchResponse({
    @Default([]) List<AppSearchResult> results,
  }) = _AppSearchResponse;

  factory AppSearchResponse.fromJson(Map<String, Object?> json) =>
      _$AppSearchResponseFromJson(json);
}

/// `POST /app-search/play` body. Sent exactly as the search that produced the
/// result was made.
@freezed
abstract class AppSearchPlayRequest with _$AppSearchPlayRequest {
  const factory AppSearchPlayRequest({
    required String q,
    required String scope,
    required String section,
    required int index,
    required String title,
  }) = _AppSearchPlayRequest;

  factory AppSearchPlayRequest.fromJson(Map<String, Object?> json) =>
      _$AppSearchPlayRequestFromJson(json);
}
