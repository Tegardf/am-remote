import 'dart:typed_data';

import 'package:flutter/services.dart' show LengthLimitingTextInputFormatter;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/models/search_result.dart';
import '../../core/providers.dart';
import '../connect/connection_indicator.dart';
import '../home/play_result.dart';
import '../home/tabs.dart';
import 'search_controller.dart';

/// Search the Apple Music catalog and play a result on the laptop.
class SearchScreen extends ConsumerStatefulWidget {
  const SearchScreen({super.key});

  @override
  ConsumerState<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends ConsumerState<SearchScreen> {
  late final TextEditingController _text;

  @override
  void initState() {
    super.initState();
    _text = TextEditingController(
      text: ref.read(searchControllerProvider).query,
    );
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _play(SearchResult result) async {
    final outcome = await ref
        .read(searchControllerProvider.notifier)
        .play(result);
    if (!mounted) return;
    if (outcome == PlayResult.started) {
      ref.read(selectedTabProvider.notifier).select(AppTab.player);
      return;
    }
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(content: Text(playFailureMessage(outcome, what: 'track')!)),
      );
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(searchControllerProvider);
    final controller = ref.read(searchControllerProvider.notifier);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Search'),
        actions: const [ConnectionIndicator()],
      ),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
            child: TextField(
              controller: _text,
              textInputAction: TextInputAction.search,
              inputFormatters: [
                LengthLimitingTextInputFormatter(
                  SearchResultsController.maxQueryLength,
                ),
              ],
              decoration: InputDecoration(
                hintText: 'Songs, albums, artists',
                prefixIcon: const Icon(Icons.search),
                suffixIcon: state.query.isEmpty
                    ? null
                    : IconButton(
                        tooltip: 'Clear',
                        icon: const Icon(Icons.close),
                        onPressed: () {
                          _text.clear();
                          controller.setQuery('');
                        },
                      ),
                border: const OutlineInputBorder(
                  borderRadius: BorderRadius.all(Radius.circular(28)),
                ),
              ),
              onChanged: controller.setQuery,
              onSubmitted: (_) => controller.submit(),
            ),
          ),
          if (state.status == SearchStatus.loading)
            const LinearProgressIndicator(),
          Expanded(
            child: _Body(state: state, onPlay: _play),
          ),
        ],
      ),
    );
  }
}

class _Body extends ConsumerWidget {
  const _Body({required this.state, required this.onPlay});

  final SearchState state;
  final Future<void> Function(SearchResult result) onPlay;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final controller = ref.read(searchControllerProvider.notifier);
    switch (state.status) {
      case SearchStatus.idle:
        return const _Message(
          icon: Icons.search,
          text: 'Type at least 2 characters to search',
        );
      case SearchStatus.loading when state.results.isEmpty:
        return const SizedBox.shrink();
      case SearchStatus.notSupported:
        return const _Message(
          icon: Icons.info_outline,
          text: "Search isn't available on this laptop yet",
        );
      case SearchStatus.notRunning:
        return _Message(
          icon: Icons.music_off,
          text: 'Open Apple Music on your laptop',
          onRetry: controller.retry,
        );
      case SearchStatus.error:
        return _Message(
          icon: Icons.error_outline,
          text: "Couldn't search. Check the connection to your laptop.",
          onRetry: controller.retry,
        );
      case SearchStatus.ready when state.results.isEmpty:
        return _Message(
          icon: Icons.search_off,
          text: 'No results for "${state.query.trim()}"',
        );
      case SearchStatus.loading:
      case SearchStatus.ready:
        return ListView.builder(
          keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
          itemCount: state.results.length,
          itemBuilder: (context, index) {
            final result = state.results[index];
            final busy = state.playingUrl == result.url;
            return ListTile(
              leading: _Thumbnail(url: result.artworkUrl),
              title: Text(result.title, maxLines: 1),
              subtitle: Text(
                [
                  result.artist,
                  result.album,
                ].where((part) => part.isNotEmpty).join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
              trailing: busy
                  ? const SizedBox.square(
                      dimension: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : null,
              enabled: state.playingUrl == null,
              onTap: () => onPlay(result),
            );
          },
        );
    }
  }
}

/// A result's artwork, loaded through the HTTPS-only [ThumbnailLoader]: the
/// app's only plain HTTP goes to the paired laptop, and a redirect to http://
/// must not be followed. Until it loads (or if it can't) a placeholder shows.
class _Thumbnail extends ConsumerStatefulWidget {
  const _Thumbnail({required this.url});

  final String url;

  static const double size = 48;

  @override
  ConsumerState<_Thumbnail> createState() => _ThumbnailState();
}

class _ThumbnailState extends ConsumerState<_Thumbnail> {
  Future<Uint8List?>? _bytes;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(_Thumbnail oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.url != widget.url) _load();
  }

  void _load() {
    _bytes = widget.url.isEmpty
        ? null
        : ref.read(thumbnailLoaderProvider).load(widget.url);
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final placeholder = ColoredBox(
      key: const ValueKey('thumbnail-placeholder'),
      color: scheme.surfaceContainerHighest,
      child: Icon(Icons.music_note, color: scheme.onSurfaceVariant),
    );
    return ClipRRect(
      borderRadius: BorderRadius.circular(8),
      child: SizedBox.square(
        dimension: _Thumbnail.size,
        child: FutureBuilder<Uint8List?>(
          future: _bytes,
          builder: (context, snapshot) {
            final bytes = snapshot.data;
            if (bytes == null) return placeholder;
            return Image.memory(
              bytes,
              key: const ValueKey('thumbnail'),
              fit: BoxFit.cover,
              gaplessPlayback: true,
              errorBuilder: (_, _, _) => placeholder,
            );
          },
        ),
      ),
    );
  }
}

class _Message extends StatelessWidget {
  const _Message({required this.icon, required this.text, this.onRetry});

  final IconData icon;
  final String text;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 56, color: theme.colorScheme.onSurfaceVariant),
            const SizedBox(height: 16),
            Text(
              text,
              textAlign: TextAlign.center,
              style: theme.textTheme.titleMedium,
            ),
            if (onRetry != null) ...[
              const SizedBox(height: 24),
              FilledButton.icon(
                onPressed: onRetry,
                icon: const Icon(Icons.refresh),
                label: const Text('Retry'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
