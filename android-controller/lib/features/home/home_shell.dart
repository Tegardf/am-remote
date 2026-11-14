import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../player/player_screen.dart';
import '../playlists/playlists_screen.dart';
import '../search/search_screen.dart';
import 'tabs.dart';

/// Player · Playlists · Search. Every tab keeps its state (scroll position,
/// search text and results) while you look at another one. The Playlists and
/// Search tabs are built the first time they are opened, so the laptop is not
/// asked for playlists until they are wanted.
class HomeShell extends ConsumerStatefulWidget {
  const HomeShell({super.key});

  @override
  ConsumerState<HomeShell> createState() => _HomeShellState();
}

class _HomeShellState extends ConsumerState<HomeShell> {
  final _opened = <AppTab>{AppTab.player};

  @override
  Widget build(BuildContext context) {
    final selected = ref.watch(selectedTabProvider);
    _opened.add(selected);

    Widget tab(AppTab t, Widget Function() build) =>
        _opened.contains(t) ? build() : const SizedBox.shrink();

    return Scaffold(
      body: IndexedStack(
        index: selected.index,
        children: [
          tab(AppTab.player, () => const PlayerScreen()),
          tab(AppTab.playlists, () => const PlaylistsScreen()),
          tab(AppTab.search, () => const SearchScreen()),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: selected.index,
        onDestinationSelected: (i) =>
            ref.read(selectedTabProvider.notifier).select(AppTab.values[i]),
        destinations: const [
          NavigationDestination(
            icon: Icon(Icons.music_note_outlined),
            selectedIcon: Icon(Icons.music_note),
            label: 'Player',
          ),
          NavigationDestination(
            icon: Icon(Icons.queue_music_outlined),
            selectedIcon: Icon(Icons.queue_music),
            label: 'Playlists',
          ),
          NavigationDestination(icon: Icon(Icons.search), label: 'Search'),
        ],
      ),
    );
  }
}
