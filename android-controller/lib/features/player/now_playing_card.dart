import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'live_controller.dart';

/// Artwork, title, artist and album of the current track.
class NowPlayingCard extends ConsumerWidget {
  const NowPlayingCard({super.key, this.artworkSize = 240});

  /// Side of the square artwork. The screen shrinks it on short displays so
  /// the buttons stay in view.
  final double artworkSize;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final live = ref.watch(liveControllerProvider);
    final player = live.player;
    final theme = Theme.of(context);

    final String title;
    final String? subtitle;
    if (player == null) {
      title = 'Waiting for the laptop…';
      subtitle = null;
    } else if (player.title.isEmpty) {
      title = 'Nothing playing';
      subtitle = null;
    } else {
      title = player.title;
      subtitle = [
        player.artist,
        player.album,
      ].where((part) => part.isNotEmpty).join(' · ');
    }

    return Column(
      children: [
        ClipRRect(
          borderRadius: BorderRadius.circular(16),
          child: SizedBox.square(
            dimension: artworkSize,
            child: live.artwork != null
                ? Image.memory(
                    live.artwork!,
                    key: const ValueKey('artwork'),
                    fit: BoxFit.cover,
                    gaplessPlayback: true,
                    errorBuilder: (_, _, _) => const _ArtworkPlaceholder(),
                  )
                : const _ArtworkPlaceholder(),
          ),
        ),
        const SizedBox(height: 20),
        Text(
          title,
          textAlign: TextAlign.center,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: theme.textTheme.titleLarge,
        ),
        if (subtitle != null && subtitle.isNotEmpty) ...[
          const SizedBox(height: 4),
          Text(
            subtitle,
            textAlign: TextAlign.center,
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
        if (player?.playlistName != null) ...[
          const SizedBox(height: 4),
          Text(
            'Playlist: ${player!.playlistName}',
            textAlign: TextAlign.center,
            style: theme.textTheme.bodySmall,
          ),
        ],
      ],
    );
  }
}

class _ArtworkPlaceholder extends StatelessWidget {
  const _ArtworkPlaceholder();

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return ColoredBox(
      key: const ValueKey('artwork-placeholder'),
      color: scheme.surfaceContainerHighest,
      child: Icon(Icons.music_note, size: 72, color: scheme.onSurfaceVariant),
    );
  }
}
