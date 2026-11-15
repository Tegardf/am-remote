import 'package:flutter/services.dart' show LengthLimitingTextInputFormatter;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/models/app_search.dart';
import '../connect/connection_indicator.dart';
import '../home/play_result.dart';
import '../home/tabs.dart';
import 'search_controller.dart';

/// Search through the Apple Music app on the laptop and play a result there.
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

  Future<void> _play(AppSearchResult result) async {
    final controller = ref.read(searchControllerProvider.notifier);
    final outcome = await controller.play(result);
    if (!mounted) return;
    if (outcome == PlayResult.started) {
      ref.read(selectedTabProvider.notifier).select(AppTab.player);
      return;
    }
    final reason = controller.lastPlayFailure;
    final message = switch (outcome) {
      PlayResult.notFound =>
        "That result isn't there any more. Searching again",
      PlayResult.notSupported =>
        "This kind of result can't be played from the phone yet",
      PlayResult.failed =>
        "Couldn't play that${reason == null ? '' : ': $reason'}",
      _ => playFailureMessage(outcome, what: 'result')!,
    };
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
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
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
            child: SizedBox(
              width: double.infinity,
              child: SegmentedButton<SearchScope>(
                segments: const [
                  ButtonSegment(
                    value: SearchScope.catalog,
                    label: Text('Catalog'),
                    icon: Icon(Icons.public),
                  ),
                  ButtonSegment(
                    value: SearchScope.library,
                    label: Text('Library'),
                    icon: Icon(Icons.library_music),
                  ),
                ],
                selected: {state.scope},
                showSelectedIcon: false,
                onSelectionChanged: (selected) =>
                    controller.setScope(selected.first),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
            child: Text(
              'Searching opens the search page in Apple Music on the laptop.',
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
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
  final Future<void> Function(AppSearchResult result) onPlay;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final theme = Theme.of(context);
    final controller = ref.read(searchControllerProvider.notifier);
    switch (state.status) {
      case SearchStatus.idle:
        return const _Message(
          icon: Icons.search,
          text: 'Type at least 2 characters to search',
        );
      // The first search of a new text takes about 4 s: say so, not just spin.
      case SearchStatus.loading when state.results.isEmpty:
        return const _Message(
          icon: Icons.hourglass_empty,
          text: 'Searching in Apple Music…',
          detail: 'This takes a few seconds.',
        );
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
          text: 'No results for "${state.searchedQuery}"',
        );
      case SearchStatus.loading:
      case SearchStatus.ready:
        return ListView(
          keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
          children: [
            for (final group in state.sections) ...[
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                child: Text(
                  group.section,
                  style: theme.textTheme.titleSmall?.copyWith(
                    color: theme.colorScheme.primary,
                  ),
                ),
              ),
              for (final result in group.results)
                ListTile(
                  title: Text(
                    result.title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: result.subtitle.isEmpty
                      ? null
                      : Text(
                          result.subtitle,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                  trailing: state.playingKey == resultKey(result)
                      ? const SizedBox.square(
                          dimension: 20,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : null,
                  enabled: state.playingKey == null,
                  onTap: () => onPlay(result),
                ),
            ],
          ],
        );
    }
  }
}

class _Message extends StatelessWidget {
  const _Message({
    required this.icon,
    required this.text,
    this.detail,
    this.onRetry,
  });

  final IconData icon;
  final String text;
  final String? detail;
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
