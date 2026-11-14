import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/agent_address.dart';
import '../../core/api_client.dart';
import '../../core/models/agent_status.dart';
import '../../core/providers.dart';

enum ConnectionStatus { connecting, connected, offline }

/// Where the app is in its connect → pair → use life cycle.
sealed class SessionState {
  const SessionState();
}

/// Reading the saved token and host.
class SessionStarting extends SessionState {
  const SessionStarting();
}

/// No valid token: show the connect / pairing flow.
class SessionUnpaired extends SessionState {
  const SessionUnpaired();
}

/// Paired with an agent. [connection] feeds the app bar indicator.
class SessionPaired extends SessionState {
  const SessionPaired({
    required this.agent,
    required this.token,
    required this.connection,
    this.status,
  });

  final AgentAddress agent;

  /// In memory only; it is persisted in secure storage by the controller.
  final String token;
  final ConnectionStatus connection;

  /// Last `GET /status` answer, if any.
  final AgentStatus? status;

  SessionPaired copyWith({
    AgentAddress? agent,
    ConnectionStatus? connection,
    AgentStatus? status,
  }) => SessionPaired(
    agent: agent ?? this.agent,
    token: token,
    connection: connection ?? this.connection,
    status: status ?? this.status,
  );
}

final sessionControllerProvider =
    NotifierProvider<SessionController, SessionState>(SessionController.new);

/// The authenticated client for the paired agent, or null while unpaired.
/// A 401 on any call sends the session back to pairing. Rebuilt only when the
/// agent or token changes, not on every connection-status change.
final apiClientProvider = Provider<ApiClient?>((ref) {
  final link = ref.watch(
    sessionControllerProvider.select(
      (s) => s is SessionPaired ? (s.agent, s.token) : null,
    ),
  );
  if (link == null) return null;
  return ref.read(apiClientFactoryProvider)(
    link.$1,
    token: link.$2,
    onUnauthorized: ref
        .read(sessionControllerProvider.notifier)
        .handleUnauthorized,
  );
});

class SessionController extends Notifier<SessionState> {
  /// Failed checks in a row before the laptop is looked for again on the
  /// network (about 30 s with the default 10 s health check).
  static const rediscoverAfterFailures = 3;

  Timer? _timer;
  bool _checking = false;
  bool _rediscovering = false;
  int _failedChecks = 0;

  /// Completes when the saved session has been restored (for tests).
  late final Future<void> ready;

  @override
  SessionState build() {
    ref.onDispose(() => _timer?.cancel());
    ready = Future.microtask(_restore);
    return const SessionStarting();
  }

  ApiClient _client(AgentAddress agent, String token) => ref.read(
    apiClientFactoryProvider,
  )(agent, token: token, onUnauthorized: handleUnauthorized);

  Future<void> _restore() async {
    final storage = ref.read(sessionStorageProvider);
    final String? token;
    final AgentAddress? agent;
    try {
      token = await storage.readToken();
      agent = await storage.readAgent();
    } catch (_) {
      if (ref.mounted) state = const SessionUnpaired();
      return;
    }
    // Pairing may have finished while we were reading storage; don't undo it.
    if (!ref.mounted || state is! SessionStarting) return;
    if (token == null || agent == null) {
      state = const SessionUnpaired();
      return;
    }
    state = SessionPaired(
      agent: agent,
      token: token,
      connection: ConnectionStatus.connecting,
    );
    _startHealthChecks();
    await checkConnection();
  }

  /// Pairing succeeded: keep the token and host, then open the app.
  Future<void> completePairing(AgentAddress agent, String token) async {
    final storage = ref.read(sessionStorageProvider);
    await storage.writeToken(token);
    await storage.writeAgent(agent);
    if (!ref.mounted) return;
    state = SessionPaired(
      agent: agent,
      token: token,
      connection: ConnectionStatus.connected,
    );
    _startHealthChecks();
    unawaited(checkConnection());
  }

  /// `GET /status` decides connected / offline; a 401 sends us back to pairing.
  Future<void> checkConnection({bool showConnecting = false}) async {
    final current = state;
    if (current is! SessionPaired || _checking) return;
    _checking = true;
    if (showConnecting) {
      state = current.copyWith(connection: ConnectionStatus.connecting);
    }
    try {
      final status = await _client(current.agent, current.token).getStatus();
      _update(ConnectionStatus.connected, status);
    } on ApiException catch (e) {
      if (e.isUnauthorized) {
        await handleUnauthorized();
      } else {
        _update(ConnectionStatus.offline, null);
      }
    } on AgentUnreachableException {
      _update(ConnectionStatus.offline, null);
    } catch (_) {
      _update(ConnectionStatus.offline, null);
    } finally {
      _checking = false;
    }
  }

  void _update(ConnectionStatus connection, AgentStatus? status) {
    final current = state;
    if (!ref.mounted || current is! SessionPaired) return;
    state = current.copyWith(connection: connection, status: status);

    if (connection == ConnectionStatus.connected) {
      _failedChecks = 0;
    } else if (connection == ConnectionStatus.offline) {
      _failedChecks++;
      if (_failedChecks % rediscoverAfterFailures == 0) {
        unawaited(_rediscover());
      }
    }
  }

  /// The saved host keeps failing: it may have a new address (DHCP). Look for
  /// the agent again and, if it is found under the same name, follow it.
  /// A manually typed address has no name, so it can't be matched: skip it.
  ///
  /// The TXT `ip` it is found with can be out of date too (contract C-004), so
  /// every address of the agent is tried (`ip` first, then the resolver's) and
  /// the first one that answers wins. If none answers, nothing changes and the
  /// next round tries again.
  Future<void> _rediscover() async {
    final current = state;
    if (current is! SessionPaired || _rediscovering) return;
    final name = current.agent.name;
    if (name == null || name.isEmpty) return;

    _rediscovering = true;
    AgentAddress? moved;
    try {
      await for (final found in ref.read(agentDiscoveryProvider).discover()) {
        if (found.name != name) continue;
        for (final host in found.hosts) {
          // The saved address is the one that keeps failing.
          if (host == current.agent.host && found.port == current.agent.port) {
            continue;
          }
          final address = AgentAddress(
            host: host,
            port: found.port,
            name: name,
          );
          if (await _answers(address, current.token)) {
            moved = address;
            break;
          }
        }
        break;
      }
    } catch (_) {
      // Discovery failed (no Wi-Fi?): the next round tries again.
    } finally {
      _rediscovering = false;
    }

    final now = state;
    if (moved == null || !ref.mounted || now is! SessionPaired) return;
    try {
      await ref.read(sessionStorageProvider).writeAgent(moved);
    } catch (_) {
      // Keep going: the new address works for this session at least.
    }
    if (!ref.mounted || state is! SessionPaired) return;
    state = (state as SessionPaired).copyWith(
      agent: moved,
      connection: ConnectionStatus.connecting,
    );
    await checkConnection();
  }

  /// Is something answering at [address]? Any HTTP answer counts, even an
  /// error: it only matters that the laptop is there. The client has no 401
  /// handler on purpose: a 401 here must not unpair the app by itself, the
  /// normal check after moving handles it.
  Future<bool> _answers(AgentAddress address, String token) async {
    try {
      await ref
          .read(apiClientFactoryProvider)(address, token: token)
          .getStatus();
      return true;
    } on ApiException {
      return true;
    } catch (_) {
      return false;
    }
  }

  /// 401: delete the token and go back to pairing. Safe to call twice.
  Future<void> handleUnauthorized() async {
    if (state is SessionUnpaired) return;
    _timer?.cancel();
    _timer = null;
    state = const SessionUnpaired();
    try {
      await ref.read(sessionStorageProvider).deleteToken();
    } catch (_) {
      // The in-memory session is already cleared; nothing else to do.
    }
  }

  void _startHealthChecks() {
    _timer?.cancel();
    final interval = ref.read(healthCheckIntervalProvider);
    if (interval == null) return;
    _timer = Timer.periodic(interval, (_) => checkConnection());
  }
}
