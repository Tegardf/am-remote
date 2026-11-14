import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/models/player_state.dart';
import '../connect/connection_indicator.dart';
import '../connect/session_controller.dart';
import 'live_controller.dart';
import 'now_playing_card.dart';
import 'player_controller.dart';
import 'progress_bar.dart';

/// Now playing, transport controls, volume and mute. What it shows comes from
/// the agent's WebSocket `state` pushes; the buttons only send commands.
class PlayerScreen extends ConsumerWidget {
  const PlayerScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final session = ref.watch(sessionControllerProvider);
    final controls = ref.watch(playerControllerProvider);
    final live = ref.watch(liveControllerProvider);
    final controller = ref.read(playerControllerProvider.notifier);

    ref.listen(playerControllerProvider, (previous, next) {
      final error = next.error;
      if (error != null && next.errorId != previous?.errorId) {
        ScaffoldMessenger.of(context)
          ..hideCurrentSnackBar()
          ..showSnackBar(SnackBar(content: Text(error)));
      }
    });

    // Apple Music is closed when a command said so (409), when the agent
    // pushed `apple_music {running:false}`, or when the last status check did.
    final running =
        live.appleMusicRunning ??
        (session is SessionPaired ? session.status?.appleMusicRunning : null);
    final notRunning = controls.notRunning || running == false;
    final offline =
        session is SessionPaired &&
        session.connection == ConnectionStatus.offline;

    return Scaffold(
      appBar: AppBar(
        title: const Text('Player'),
        actions: const [ConnectionIndicator()],
      ),
      body: Column(
        children: [
          if (offline) const _OfflineBanner(),
          Expanded(
            child: notRunning
                ? _NotRunning(onRetry: controller.retry)
                : _Controls(
                    laptop: session is SessionPaired
                        ? session.agent.displayName
                        : '',
                    playing: live.player?.status == PlaybackStatus.playing,
                    state: controls,
                    controller: controller,
                  ),
          ),
        ],
      ),
    );
  }
}

class _OfflineBanner extends StatelessWidget {
  const _OfflineBanner();

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Material(
      color: scheme.errorContainer,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
        child: Row(
          children: [
            Icon(Icons.cloud_off, size: 20, color: scheme.onErrorContainer),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                'No connection to your laptop. Trying again…',
                style: TextStyle(color: scheme.onErrorContainer),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _NotRunning extends StatelessWidget {
  const _NotRunning({required this.onRetry});

  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.music_off,
              size: 64,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            const SizedBox(height: 16),
            Text(
              'Open Apple Music on your laptop',
              textAlign: TextAlign.center,
              style: theme.textTheme.titleLarge,
            ),
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: onRetry,
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
            ),
          ],
        ),
      ),
    );
  }
}

class _Controls extends StatelessWidget {
  const _Controls({
    required this.laptop,
    required this.playing,
    required this.state,
    required this.controller,
  });

  final String laptop;
  final bool playing;
  final PlayerControlsState state;
  final PlayerController controller;

  /// Everything in the list except the artwork: paddings, the laptop name,
  /// the track text, the progress bar and the button row (a little generous,
  /// so a two-line title still fits).
  static const double _fixedContentHeight = 350;
  static const double _minArtwork = 96;
  static const double _maxArtwork = 260;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final idle = !state.busy;

    return Column(
      children: [
        Expanded(
          // The artwork gives way to the buttons on short screens; the list
          // scrolls only if even the smallest artwork doesn't fit.
          child: LayoutBuilder(
            builder: (context, constraints) {
              final artwork = (constraints.maxHeight - _fixedContentHeight)
                  .clamp(_minArtwork, _maxArtwork);
              return ListView(
                padding: const EdgeInsets.fromLTRB(24, 16, 24, 8),
                children: [
                  Center(
                    child: Text(laptop, style: theme.textTheme.labelLarge),
                  ),
                  const SizedBox(height: 16),
                  Center(child: NowPlayingCard(artworkSize: artwork)),
                  const SizedBox(height: 8),
                  const ProgressBar(),
                  const SizedBox(height: 8),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                    children: [
                      IconButton.filledTonal(
                        tooltip: 'Previous',
                        iconSize: 40,
                        onPressed: idle ? controller.previous : null,
                        icon: const Icon(Icons.skip_previous),
                      ),
                      IconButton.filled(
                        tooltip: 'Play / pause',
                        iconSize: 56,
                        padding: const EdgeInsets.all(20),
                        onPressed: idle ? controller.toggle : null,
                        icon: Icon(playing ? Icons.pause : Icons.play_arrow),
                      ),
                      IconButton.filledTonal(
                        tooltip: 'Next',
                        iconSize: 40,
                        onPressed: idle ? controller.next : null,
                        icon: const Icon(Icons.skip_next),
                      ),
                    ],
                  ),
                ],
              );
            },
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 0, 24, 16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  IconButton(
                    tooltip: state.muted ? 'Unmute' : 'Mute',
                    onPressed: idle && state.volumeLoaded
                        ? controller.toggleMute
                        : null,
                    icon: Icon(
                      state.muted ? Icons.volume_off : Icons.volume_up,
                    ),
                  ),
                  Expanded(
                    child: Slider(
                      value: state.volume.toDouble(),
                      min: 0,
                      max: 100,
                      divisions: 100,
                      label: '${state.volume}',
                      onChangeStart: state.volumeLoaded
                          ? (_) => controller.volumeDragStart()
                          : null,
                      onChanged: state.volumeLoaded
                          ? (v) => controller.volumeChanged(v.round())
                          : null,
                      onChangeEnd: state.volumeLoaded
                          ? (v) => controller.volumeDragEnd(v.round())
                          : null,
                    ),
                  ),
                  SizedBox(
                    width: 40,
                    child: Text(
                      '${state.volume}',
                      textAlign: TextAlign.end,
                      style: theme.textTheme.bodyLarge,
                    ),
                  ),
                ],
              ),
              // Apple Music is running, but Windows has no audio session for it
              // yet: only the volume is out of reach, so say why.
              if (state.volumeUnavailable)
                Text(
                  'Volume works once Apple Music has played something',
                  textAlign: TextAlign.center,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
            ],
          ),
        ),
      ],
    );
  }
}
