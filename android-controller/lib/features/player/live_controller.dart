import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/modes.dart';
import '../../core/models/player_state.dart';
import '../../core/models/volume.dart';
import '../../core/models/ws_message.dart';
import '../../core/ws_client.dart';
import '../connect/session_controller.dart';

/// How long to wait before asking for the same artwork again after a failed
/// request, so a broken endpoint isn't hit on every push. Tests shorten it.
final artworkRetryDelayProvider = Provider<Duration>(
  (ref) => const Duration(seconds: 5),
);

/// Builds the WebSocket client for a URL. Tests override this.
typedef WsClientFactory = WsClient Function(Uri url);

final wsClientFactoryProvider = Provider<WsClientFactory>(
  (ref) =>
      (url) => WsClient(url: url),
);

/// The live socket of the paired agent, or null while unpaired. Rebuilt only
/// when the agent or token changes.
final wsClientProvider = Provider<WsClient?>((ref) {
  final link = ref.watch(
    sessionControllerProvider.select(
      (s) => s is SessionPaired ? (s.agent, s.token) : null,
    ),
  );
  if (link == null) return null;
  final client = ref.read(wsClientFactoryProvider)(
    link.$1.webSocketUri(link.$2),
  );
  ref.onDispose(client.dispose);
  client.start();
  return client;
});

/// Everything the agent pushes. The WebSocket `state` events are the source of
/// truth for the Player UI: the buttons only send HTTP commands.
class LiveState {
  const LiveState({
    this.player,
    this.playerReceivedAt,
    this.appleMusicRunning,
    this.artwork,
    this.artworkMissing = false,
    this.link = WsLinkStatus.disconnected,
    this.volume,
    this.volumeSeq = 0,
    this.modes,
    this.modesSeq = 0,
  });

  /// The latest `PlayerState`, and when it arrived (for the progress bar).
  final PlayerState? player;
  final DateTime? playerReceivedAt;

  /// From `apple_music` events; null until the agent has said anything.
  final bool? appleMusicRunning;

  /// JPEG bytes for `player.artworkRev`; null while loading or when none.
  final Uint8List? artwork;

  /// The agent has no artwork for this track (404): show the placeholder.
  final bool artworkMissing;

  final WsLinkStatus link;

  /// The latest pushed `volume`; [volumeSeq] counts them so a repeated value
  /// still counts as news.
  final Volume? volume;
  final int volumeSeq;

  /// The latest pushed shuffle and repeat state (MVP 4), counted like
  /// [volumeSeq].
  final Modes? modes;
  final int modesSeq;

  LiveState copyWith({
    PlayerState? player,
    DateTime? playerReceivedAt,
    bool? appleMusicRunning,
    Uint8List? artwork,
    bool clearArtwork = false,
    bool? artworkMissing,
    WsLinkStatus? link,
    Volume? volume,
    int? volumeSeq,
    Modes? modes,
    int? modesSeq,
  }) => LiveState(
    player: player ?? this.player,
    playerReceivedAt: playerReceivedAt ?? this.playerReceivedAt,
    appleMusicRunning: appleMusicRunning ?? this.appleMusicRunning,
    artwork: clearArtwork ? null : (artwork ?? this.artwork),
    artworkMissing: artworkMissing ?? this.artworkMissing,
    link: link ?? this.link,
    volume: volume ?? this.volume,
    volumeSeq: volumeSeq ?? this.volumeSeq,
    modes: modes ?? this.modes,
    modesSeq: modesSeq ?? this.modesSeq,
  );
}

final liveControllerProvider = NotifierProvider<LiveController, LiveState>(
  LiveController.new,
);

class LiveController extends Notifier<LiveState> {
  StreamSubscription<WsEvent>? _events;
  StreamSubscription<WsLinkStatus>? _links;

  /// The `artwork_rev` that was fetched last (or is being fetched).
  int? _artworkRev;

  @override
  LiveState build() {
    final client = ref.watch(wsClientProvider);
    _artworkRev = null;
    ref.onDispose(() {
      _events?.cancel();
      _links?.cancel();
    });
    if (client == null) return const LiveState();
    _events = client.events.listen(_onEvent);
    _links = client.statuses.listen((link) {
      if (!ref.mounted) return;
      state = state.copyWith(link: link);
      // The socket is the first to notice a laptop that went away, or a token
      // that was revoked (a 401 on the handshake). Check right now instead of
      // waiting for the next health check: it flips the indicator to offline,
      // or sends the app back to pairing on a 401.
      if (link == WsLinkStatus.disconnected) {
        unawaited(
          ref.read(sessionControllerProvider.notifier).checkConnection(),
        );
      }
    });
    // The agent pushes `state` on connect, but ask once as well so the screen
    // doesn't wait on the socket (or miss a push sent before we listened).
    Future.microtask(_loadInitialState);
    return LiveState(link: client.status);
  }

  Future<void> _loadInitialState() async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    try {
      final player = await api.getPlayerState();
      // A pushed state that arrived meanwhile is newer.
      if (ref.mounted && state.player == null) _applyState(player);
    } on ApiException catch (e) {
      if (ref.mounted && e.code == ApiErrorCode.appleMusicNotRunning) {
        state = state.copyWith(appleMusicRunning: false);
      }
    } catch (_) {
      // Offline or not ready: the socket will deliver the state.
    }
  }

  void _onEvent(WsEvent event) {
    if (!ref.mounted) return;
    switch (event) {
      case StateEvent(state: final player):
        _applyState(player);
      case VolumeEvent(:final volume):
        state = state.copyWith(volume: volume, volumeSeq: state.volumeSeq + 1);
      case AppleMusicEvent(:final running):
        state = state.copyWith(appleMusicRunning: running);
      case ModesEvent(:final modes):
        state = state.copyWith(modes: modes, modesSeq: state.modesSeq + 1);
      case PongEvent():
        break;
    }
  }

  void _applyState(PlayerState player) {
    state = state.copyWith(player: player, playerReceivedAt: DateTime.now());
    // Artwork is fetched again only when its revision changes, or when the
    // last attempt for this revision failed (and a pause has passed).
    final rev = player.artworkRev;
    if (rev != _artworkRev && !_failedRecently(rev)) {
      _artworkRev = rev;
      _fetchArtwork(rev);
    }
  }

  int? _failedRev;
  DateTime? _failedAt;

  bool _failedRecently(int rev) =>
      rev == _failedRev &&
      _failedAt != null &&
      DateTime.now().difference(_failedAt!) <
          ref.read(artworkRetryDelayProvider);

  Future<void> _fetchArtwork(int rev) async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    Uint8List? bytes;
    try {
      bytes = await api.getArtwork();
    } catch (_) {
      // Not "no artwork" (that is a 404, answered with null): the laptop was
      // unreachable or errored. Keep what is shown and try again on a later
      // push or after a reconnect.
      if (ref.mounted && _artworkRev == rev) {
        _artworkRev = null;
        _failedRev = rev;
        _failedAt = DateTime.now();
      }
      return;
    }
    // A newer revision started meanwhile: this answer is stale.
    if (!ref.mounted || _artworkRev != rev) return;
    _failedRev = null;
    state = bytes == null
        ? state.copyWith(clearArtwork: true, artworkMissing: true)
        : state.copyWith(artwork: bytes, artworkMissing: false);
  }
}
