import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'agent_address.dart';
import 'api_client.dart';
import 'discovery.dart';
import 'session_storage.dart';
import 'thumbnail_loader.dart';

/// Builds an [ApiClient] for an agent. Tests override this with a fake.
typedef ApiClientFactory = ApiClient Function(
  AgentAddress agent, {
  String? token,
  UnauthorizedCallback? onUnauthorized,
});

final apiClientFactoryProvider = Provider<ApiClientFactory>(
  (ref) =>
      (agent, {token, onUnauthorized}) => DioApiClient(
        agent: agent,
        token: token,
        onUnauthorized: onUnauthorized,
      ),
);

/// Loads search-result artwork over HTTPS only. Tests override this.
final thumbnailLoaderProvider = Provider<ThumbnailLoader>(
  (ref) => HttpsThumbnailLoader(),
);

final sessionStorageProvider = Provider<SessionStorage>(
  (ref) => DeviceSessionStorage(),
);

final agentDiscoveryProvider = Provider<AgentDiscovery>(
  (ref) => const NsdAgentDiscovery(),
);

/// How often a paired session re-checks the agent (offline → connected).
/// Tests override this with null so no timers are left running.
final healthCheckIntervalProvider = Provider<Duration?>(
  (ref) => const Duration(seconds: 10),
);

/// Default for the pairing form's device name: the phone model.
final deviceNameProvider = FutureProvider<String>((ref) async {
  try {
    final info = await DeviceInfoPlugin().androidInfo;
    final model = info.model;
    return model.isEmpty ? 'Android phone' : model;
  } catch (_) {
    return 'Android phone';
  }
});
