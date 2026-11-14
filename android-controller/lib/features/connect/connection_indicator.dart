import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'session_controller.dart';

/// App bar badge: connected / connecting / offline. Tap to re-check now.
class ConnectionIndicator extends ConsumerWidget {
  const ConnectionIndicator({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(sessionControllerProvider);
    if (session is! SessionPaired) return const SizedBox.shrink();

    final (label, color) = switch (session.connection) {
      ConnectionStatus.connected => ('Connected', Colors.green),
      ConnectionStatus.connecting => ('Connecting…', Colors.amber),
      ConnectionStatus.offline => ('Offline', Colors.red),
    };

    return InkWell(
      borderRadius: BorderRadius.circular(16),
      onTap: () => ref
          .read(sessionControllerProvider.notifier)
          .checkConnection(showConnecting: true),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.circle, size: 10, color: color),
            const SizedBox(width: 6),
            Text(label),
          ],
        ),
      ),
    );
  }
}
