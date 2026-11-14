import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/discovery.dart';
import '../../core/providers.dart';

class DiscoveryState {
  const DiscoveryState({
    this.agents = const [],
    this.searching = true,
    this.failed = false,
  });

  final List<DiscoveredAgent> agents;
  final bool searching;

  /// Discovery itself failed (e.g. no Wi-Fi); manual entry still works.
  final bool failed;

  DiscoveryState copyWith({
    List<DiscoveredAgent>? agents,
    bool? searching,
    bool? failed,
  }) => DiscoveryState(
    agents: agents ?? this.agents,
    searching: searching ?? this.searching,
    failed: failed ?? this.failed,
  );
}

final discoveryControllerProvider =
    NotifierProvider.autoDispose<DiscoveryController, DiscoveryState>(
      DiscoveryController.new,
    );

class DiscoveryController extends Notifier<DiscoveryState> {
  StreamSubscription<DiscoveredAgent>? _subscription;

  @override
  DiscoveryState build() {
    ref.onDispose(() => _subscription?.cancel());
    Future.microtask(search);
    return const DiscoveryState();
  }

  /// (Re)starts the search; found agents are listed as they appear.
  Future<void> search() async {
    await _subscription?.cancel();
    if (!ref.mounted) return;
    state = const DiscoveryState();
    _subscription = ref
        .read(agentDiscoveryProvider)
        .discover()
        .listen(
          (agent) {
            if (!ref.mounted || state.agents.contains(agent)) return;
            state = state.copyWith(agents: [...state.agents, agent]);
          },
          onError: (Object _) {
            if (ref.mounted) {
              state = state.copyWith(searching: false, failed: true);
            }
          },
          onDone: () {
            if (ref.mounted) state = state.copyWith(searching: false);
          },
        );
  }
}
