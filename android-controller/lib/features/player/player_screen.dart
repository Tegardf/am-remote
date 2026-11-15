import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/models/modes.dart';
import '../../core/models/player_state.dart';
import '../../core/models/queue_item.dart';
import '../connect/connection_indicator.dart';
import '../connect/session_controller.dart';
import '../home/play_result.dart';
import 'live_controller.dart';
import 'modes_controller.dart';
import 'now_playing_card.dart';
import 'player_controller.dart';
import 'progress.dart';
import 'progress_bar.dart';
import 'queue_controller.dart';

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
    // Shuffle/repeat and the queue say the same when they are refused with 409.
    final modesNotRunning = ref.watch(
      modesControllerProvider.select((s) => s.notRunning),
    );
    final queueNotRunning = ref.watch(
      queueControllerProvider.select((s) => s.status == QueueStatus.notRunning),
    );
    final notRunning =
        controls.notRunning ||
        modesNotRunning ||
        queueNotRunning ||
        running == false;
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
                ? _NotRunning(
                    onRetry: () {
                      controller.retry();
                      ref.read(modesControllerProvider.notifier).load();
                      ref.read(queueControllerProvider.notifier).load();
                    },
                  )
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

class _Controls extends ConsumerWidget {
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
  /// the track text, the progress bar, the transport buttons and the shuffle /
  /// repeat row (a little generous, so a two-line title still fits). The Up
  /// Next list is below that and scrolls with it.
  static const double _fixedContentHeight = 420;
  static const double _minArtwork = 96;
  static const double _maxArtwork = 260;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final idle = !state.busy;

    return Column(
      children: [
        Expanded(
          // The artwork gives way to the buttons on short screens; the list
          // scrolls only if even the smallest artwork doesn't fit. Pulling it
          // down reads the Up Next list again.
          child: LayoutBuilder(
            builder: (context, constraints) {
              final artwork = (constraints.maxHeight - _fixedContentHeight)
                  .clamp(_minArtwork, _maxArtwork);
              return RefreshIndicator(
                onRefresh: ref.read(queueControllerProvider.notifier).load,
                child: ListView(
                  physics: const AlwaysScrollableScrollPhysics(),
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
                    const SizedBox(height: 8),
                    const _ModesRow(),
                    const SizedBox(height: 8),
                    const _UpNext(),
                  ],
                ),
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

/// Shuffle and repeat. They show what the agent answered or pushed, never a
/// guess, and are off while that is unknown, stale or on its way.
class _ModesRow extends ConsumerWidget {
  const _ModesRow();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final state = ref.watch(modesControllerProvider);
    final controller = ref.read(modesControllerProvider.notifier);

    ref.listen(modesControllerProvider.select((s) => s.errorId), (
      previous,
      next,
    ) {
      final error = ref.read(modesControllerProvider).error;
      if (error == null) return;
      ScaffoldMessenger.of(context)
        ..hideCurrentSnackBar()
        ..showSnackBar(SnackBar(content: Text(error)));
    });

    final modes = state.modes ?? const Modes();
    final enabled = state.usable;
    final (repeatIcon, repeatLabel) = switch (modes.repeat) {
      RepeatSetting.off => (Icons.repeat, 'Repeat off'),
      RepeatSetting.all => (Icons.repeat, 'Repeat all'),
      RepeatSetting.one => (Icons.repeat_one, 'Repeat one'),
    };

    return Column(
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            IconButton.filledTonal(
              tooltip: modes.shuffle ? 'Shuffle on' : 'Shuffle off',
              isSelected: modes.shuffle,
              onPressed: enabled ? controller.toggleShuffle : null,
              icon: const Icon(Icons.shuffle),
            ),
            const SizedBox(width: 24),
            IconButton.filledTonal(
              tooltip: repeatLabel,
              isSelected: modes.repeat != RepeatSetting.off,
              onPressed: enabled ? controller.cycleRepeat : null,
              icon: Icon(repeatIcon),
            ),
          ],
        ),
        if (state.unsupported)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              'Not available on this laptop',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
      ],
    );
  }
}

/// The Up Next list: about the next 20 songs. Tap a row to play it.
class _UpNext extends ConsumerWidget {
  const _UpNext();

  Future<void> _play(
    BuildContext context,
    WidgetRef ref,
    QueueItem item,
  ) async {
    final controller = ref.read(queueControllerProvider.notifier);
    final result = await controller.play(item);
    if (!context.mounted || result == PlayResult.started) return;
    final message = switch (result) {
      PlayResult.notFound => 'The queue changed',
      PlayResult.notSupported => "This song can't be played from the phone",
      PlayResult.failed =>
        "Couldn't play that song"
            '${controller.lastPlayFailure == null ? '' : ': ${controller.lastPlayFailure}'}',
      _ => playFailureMessage(result, what: 'song')!,
    };
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final state = ref.watch(queueControllerProvider);
    final controller = ref.read(queueControllerProvider.notifier);
    final muted = theme.textTheme.bodySmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );

    final body = switch (state.status) {
      QueueStatus.loading => const Padding(
        padding: EdgeInsets.symmetric(vertical: 16),
        child: Center(child: CircularProgressIndicator()),
      ),
      QueueStatus.notSupported => Padding(
        padding: const EdgeInsets.symmetric(vertical: 16),
        child: Text("Queue isn't available on this laptop", style: muted),
      ),
      // The whole screen shows the "Open Apple Music" state meanwhile.
      QueueStatus.notRunning => const SizedBox.shrink(),
      QueueStatus.error => Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: Row(
          children: [
            Expanded(child: Text("Couldn't load the queue", style: muted)),
            TextButton(onPressed: controller.load, child: const Text('Retry')),
          ],
        ),
      ),
      QueueStatus.ready =>
        state.items.isEmpty
            ? Padding(
                padding: const EdgeInsets.symmetric(vertical: 16),
                child: Text('Nothing is queued', style: muted),
              )
            : Column(
                children: [
                  for (var i = 0; i < state.items.length; i++) ...[
                    // A small divider where Apple Music's own picks start.
                    if (state.items[i].isAutoplay &&
                        (i == 0 || !state.items[i - 1].isAutoplay))
                      Padding(
                        padding: const EdgeInsets.fromLTRB(0, 12, 0, 4),
                        child: Row(
                          children: [
                            Text('AutoPlay', style: muted),
                            const SizedBox(width: 8),
                            const Expanded(child: Divider()),
                          ],
                        ),
                      ),
                    _QueueRow(
                      item: state.items[i],
                      busy: state.playingIndex == state.items[i].index,
                      enabled: state.playingIndex == null,
                      onTap: () => _play(context, ref, state.items[i]),
                    ),
                  ],
                ],
              ),
    };

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Up Next', style: theme.textTheme.titleMedium),
        Text('Shows about the next 20 songs', style: muted),
        const SizedBox(height: 4),
        body,
      ],
    );
  }
}

class _QueueRow extends StatelessWidget {
  const _QueueRow({
    required this.item,
    required this.busy,
    required this.enabled,
    required this.onTap,
  });

  final QueueItem item;
  final bool busy;
  final bool enabled;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      dense: true,
      enabled: enabled,
      title: Text(item.title, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: item.artist.isEmpty
          ? null
          : Text(item.artist, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: busy
          ? const SizedBox.square(
              dimension: 20,
              child: CircularProgressIndicator(strokeWidth: 2),
            )
          : item.durationMs > 0
          ? Text(formatPosition(item.durationMs))
          : null,
      onTap: onTap,
    );
  }
}
