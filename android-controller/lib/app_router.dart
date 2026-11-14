import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'features/connect/connect_screen.dart';
import 'features/connect/session_controller.dart';
import 'features/home/home_shell.dart';
import 'features/player/media_notification.dart';

/// Picks the screen for the current session: connect, or the app with its
/// bottom navigation (Player · Playlists · Search).
class AppRouter extends ConsumerWidget {
  const AppRouter({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Keeps the media notification in step with the laptop for as long as the
    // app is open.
    ref.watch(mediaNotificationProvider);
    return switch (ref.watch(sessionControllerProvider)) {
      SessionStarting() => const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      ),
      SessionUnpaired() => const ConnectScreen(),
      SessionPaired() => const HomeShell(),
    };
  }
}
