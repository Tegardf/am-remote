import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../connect/connection_indicator.dart';
import '../home/play_result.dart';
import '../home/tabs.dart';
import 'playlists_controller.dart';

/// The saved playlists. Tap one to play it on the laptop.
class PlaylistsScreen extends ConsumerWidget {
  const PlaylistsScreen({super.key});

  Future<void> _play(BuildContext context, WidgetRef ref, String id) async {
    final result = await ref
        .read(playlistsControllerProvider.notifier)
        .play(id);
    if (!context.mounted) return;
    if (result == PlayResult.started) {
      ref.read(selectedTabProvider.notifier).select(AppTab.player);
      return;
    }
    final message = playFailureMessage(result, what: 'playlist')!;
    final reason = result == PlayResult.failed
        ? ref.read(playlistsControllerProvider.notifier).lastPlayFailure
        : null;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(content: Text(reason == null ? message : '$message: $reason')),
      );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(playlistsControllerProvider);
    final controller = ref.read(playlistsControllerProvider.notifier);

    // A refresh failed but the list is still there: say so once.
    ref.listen(playlistsControllerProvider.select((s) => s.refreshErrorId), (
      previous,
      next,
    ) {
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(
          const SnackBar(content: Text("Couldn't refresh your playlists")),
        );
    });

    return Scaffold(
      appBar: AppBar(
        title: const Text('Playlists'),
        actions: const [ConnectionIndicator()],
      ),
      body: switch (state.status) {
        PlaylistsStatus.loading => const Center(
          child: CircularProgressIndicator(),
        ),
        PlaylistsStatus.ready =>
          state.items.isEmpty
              ? _Message(
                  icon: Icons.queue_music,
                  text: 'No playlists yet',
                  onRefresh: controller.load,
                )
              : RefreshIndicator(
                  onRefresh: controller.load,
                  child: ListView.builder(
                    physics: const AlwaysScrollableScrollPhysics(),
                    itemCount: state.items.length,
                    itemBuilder: (context, index) {
                      final playlist = state.items[index];
                      final busy = state.playingId == playlist.id;
                      return ListTile(
                        leading: const Icon(Icons.queue_music),
                        title: Text(playlist.name),
                        trailing: busy
                            ? const SizedBox.square(
                                dimension: 20,
                                child: CircularProgressIndicator(
                                  strokeWidth: 2,
                                ),
                              )
                            : const Icon(Icons.play_arrow),
                        enabled: state.playingId == null,
                        onTap: () => _play(context, ref, playlist.id),
                      );
                    },
                  ),
                ),
        PlaylistsStatus.notSupported => _Message(
          icon: Icons.info_outline,
          text: "Playlists aren't available on this laptop yet",
          onRefresh: controller.load,
        ),
        PlaylistsStatus.notRunning => _Message(
          icon: Icons.music_off,
          text: 'Open Apple Music on your laptop',
          onRefresh: controller.load,
          retry: controller.load,
        ),
        PlaylistsStatus.error => _Message(
          icon: Icons.error_outline,
          text: "Couldn't load your playlists",
          detail: state.errorDetail,
          onRefresh: controller.load,
          retry: controller.load,
        ),
      },
    );
  }
}

/// A centred message that can still be pulled down to refresh.
class _Message extends StatelessWidget {
  const _Message({
    required this.icon,
    required this.text,
    required this.onRefresh,
    this.retry,
    this.detail,
  });

  final IconData icon;
  final String text;

  /// Why it failed, in smaller text under [text].
  final String? detail;
  final Future<void> Function() onRefresh;
  final Future<void> Function()? retry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return RefreshIndicator(
      onRefresh: onRefresh,
      child: LayoutBuilder(
        builder: (context, constraints) => SingleChildScrollView(
          physics: const AlwaysScrollableScrollPhysics(),
          child: ConstrainedBox(
            constraints: BoxConstraints(minHeight: constraints.maxHeight),
            child: Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      icon,
                      size: 56,
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                    const SizedBox(height: 16),
                    Text(
                      text,
                      textAlign: TextAlign.center,
                      style: theme.textTheme.titleMedium,
                    ),
                    if (detail != null) ...[
                      const SizedBox(height: 8),
                      Text(
                        detail!,
                        textAlign: TextAlign.center,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                    if (retry != null) ...[
                      const SizedBox(height: 24),
                      FilledButton.icon(
                        onPressed: retry,
                        icon: const Icon(Icons.refresh),
                        label: const Text('Retry'),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
