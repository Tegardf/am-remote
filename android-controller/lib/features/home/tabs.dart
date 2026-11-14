import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../connect/session_controller.dart';

enum AppTab { player, playlists, search }

/// The tab shown in the bottom navigation. Other screens switch it, for
/// example to the Player tab after starting a track.
final selectedTabProvider = NotifierProvider<SelectedTab, AppTab>(
  SelectedTab.new,
);

class SelectedTab extends Notifier<AppTab> {
  @override
  AppTab build() {
    // Leaving the app (a 401 sends it back to pairing) and coming back
    // starts on the Player tab again.
    ref.watch(sessionControllerProvider.select((s) => s is SessionPaired));
    return AppTab.player;
  }

  void select(AppTab tab) => state = tab;
}
