import 'package:freezed_annotation/freezed_annotation.dart';

part 'playlist.freezed.dart';
part 'playlist.g.dart';

/// One entry of `GET /playlists`.
@freezed
abstract class Playlist with _$Playlist {
  const factory Playlist({required String id, required String name}) =
      _Playlist;

  factory Playlist.fromJson(Map<String, Object?> json) =>
      _$PlaylistFromJson(json);
}

/// `POST /play/playlist` body.
@freezed
abstract class PlayPlaylistRequest with _$PlayPlaylistRequest {
  const factory PlayPlaylistRequest({required String id}) =
      _PlayPlaylistRequest;

  factory PlayPlaylistRequest.fromJson(Map<String, Object?> json) =>
      _$PlayPlaylistRequestFromJson(json);
}
