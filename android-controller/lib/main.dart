import 'package:audio_service/audio_service.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import 'app_router.dart';
import 'features/player/media_notification.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // The media notification is a bonus: if it can't start, the app still works.
  AmRemoteAudioHandler? handler;
  try {
    handler = await AudioService.init(
      builder: AmRemoteAudioHandler.new,
      config: const AudioServiceConfig(
        androidNotificationChannelId: 'dev.amremote.am_remote.playback',
        androidNotificationChannelName: 'Now playing on your laptop',
        // Keeps the service in the foreground while paused, so that resuming
        // from the notification never needs to restart it from the background.
        androidStopForegroundOnPause: false,
      ),
    );
  } catch (e) {
    if (kDebugMode) debugPrint('Media notification unavailable: $e');
  }

  runApp(
    ProviderScope(
      overrides: [audioHandlerProvider.overrideWithValue(handler)],
      child: const AmRemoteApp(),
    ),
  );
}

class AmRemoteApp extends StatelessWidget {
  const AmRemoteApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'AM Remote',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.pink,
          brightness: Brightness.light,
        ),
      ),
      darkTheme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: Colors.pink,
          brightness: Brightness.dark,
        ),
      ),
      home: const AppRouter(),
    );
  }
}
