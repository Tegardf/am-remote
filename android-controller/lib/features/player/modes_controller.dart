import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api_client.dart';
import '../../core/models/modes.dart';
import '../../core/ws_client.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';
import 'live_controller.dart';

/// Shuffle and repeat (MVP 4).
class ModesState {
  const ModesState({
    this.modes,
    this.stale = false,
    this.unsupported = false,
    this.notRunning = false,
    this.busy = false,
    this.error,
    this.errorId = 0,
  });

  /// The last state the agent told us, or null before the first answer.
  final Modes? modes;

  /// [modes] may be out of date (Apple Music went away, or a read failed with
  /// 409): the buttons stay off until the agent answers again.
  final bool stale;

  /// The agent can't read or change them here (`501`, or it doesn't know the
  /// route at all): only these two buttons are off.
  final bool unsupported;

  /// The agent answered `APPLE_MUSIC_NOT_RUNNING`.
  final bool notRunning;

  /// A change is on its way: nothing is shown before the agent's answer.
  final bool busy;

  /// A short message for a snackbar; [errorId] changes with every new one.
  final String? error;
  final int errorId;

  /// The buttons may be pressed.
  bool get usable => modes != null && !stale && !unsupported && !busy;

  ModesState copyWith({
    Modes? modes,
    bool? stale,
    bool? unsupported,
    bool? notRunning,
    bool? busy,
    String? error,
    int? errorId,
  }) => ModesState(
    modes: modes ?? this.modes,
    stale: stale ?? this.stale,
    unsupported: unsupported ?? this.unsupported,
    notRunning: notRunning ?? this.notRunning,
    busy: busy ?? this.busy,
    error: error ?? this.error,
    errorId: errorId ?? this.errorId,
  );
}

final modesControllerProvider = NotifierProvider<ModesController, ModesState>(
  ModesController.new,
);

/// Reads and changes shuffle and repeat. The buttons show what the agent
/// answered (or pushed), never a guess.
class ModesController extends Notifier<ModesState> {
  /// Counts reads and pushes: an older answer never overwrites a newer one.
  int _loadId = 0;

  @override
  ModesState build() {
    // A new client means a new pairing: start fresh.
    ref.watch(apiClientProvider);

    // The agent pushes `modes` on connect and when it sees a change (it polls
    // Apple Music's buttons, so a change made on the laptop arrives late).
    ref.listen(liveControllerProvider.select((s) => s.modesSeq), (_, _) {
      final pushed = ref.read(liveControllerProvider).modes;
      if (pushed == null) return;
      _loadId++; // any read that started earlier is older than this
      state = state.copyWith(
        modes: pushed,
        stale: false,
        unsupported: false,
        notRunning: false,
      );
    });

    // Apple Music was started: read again. When it stops, what we know is stale.
    ref.listen(liveControllerProvider.select((s) => s.appleMusicRunning), (
      previous,
      next,
    ) {
      if (next == true && previous != true) {
        load();
      } else if (next == false && previous != false) {
        state = state.copyWith(stale: true);
      }
    });

    // The socket reconnected: the agent may have missed a change meanwhile, so
    // read again (the first connection is not a reconnect).
    var wasConnected =
        ref.read(liveControllerProvider).link == WsLinkStatus.connected;
    ref.listen(liveControllerProvider.select((s) => s.link), (previous, next) {
      if (next != WsLinkStatus.connected) return;
      if (wasConnected && previous != WsLinkStatus.connected) load();
      wasConnected = true;
    });

    Future.microtask(load);
    return const ModesState();
  }

  /// `GET /player/modes`.
  Future<void> load() async {
    final api = ref.read(apiClientProvider);
    if (api == null) return;
    final id = ++_loadId;
    try {
      final modes = await api.getModes();
      if (!ref.mounted || id != _loadId) return;
      state = state.copyWith(
        modes: modes,
        stale: false,
        unsupported: false,
        notRunning: false,
      );
    } catch (e) {
      if (!ref.mounted || id != _loadId) return;
      _fail(e, quiet: true);
    }
  }

  Future<void> toggleShuffle() {
    final modes = state.modes;
    if (modes == null) return Future.value();
    return _set((api) => api.setModes(shuffle: !modes.shuffle));
  }

  /// One press: off → all → one → off.
  Future<void> cycleRepeat() {
    final modes = state.modes;
    if (modes == null) return Future.value();
    return _set((api) => api.setModes(repeat: modes.repeat.next));
  }

  Future<void> _set(Future<Modes> Function(ApiClient api) send) async {
    final api = ref.read(apiClientProvider);
    if (api == null || !state.usable) return;
    state = state.copyWith(busy: true);
    try {
      final modes = await send(api);
      if (!ref.mounted) return;
      _loadId++;
      state = state.copyWith(
        modes: modes,
        busy: false,
        stale: false,
        unsupported: false,
        notRunning: false,
      );
    } catch (e) {
      _fail(e, clearBusy: true);
    }
  }

  void _fail(Object e, {bool quiet = false, bool clearBusy = false}) {
    if (!ref.mounted) return;
    var next = clearBusy ? state.copyWith(busy: false) : state;
    String? message;
    if (e is ApiException) {
      switch (e.code) {
        // 501, or an agent that doesn't know these routes (404).
        case ApiErrorCode.notSupported || ApiErrorCode.notFound:
          next = next.copyWith(unsupported: true);
        case ApiErrorCode.appleMusicNotRunning:
          next = next.copyWith(notRunning: true, stale: true);
        case ApiErrorCode.unauthorized:
          break; // the client already sent the app back to pairing
        default:
          message = "Couldn't change that on your laptop";
      }
    } else {
      message = failureReason(e);
    }
    if (message != null && !quiet) {
      next = next.copyWith(error: message, errorId: state.errorId + 1);
    }
    state = next;
  }
}
