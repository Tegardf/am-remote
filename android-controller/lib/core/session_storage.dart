import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'agent_address.dart';

/// Persists the pairing token (secret) and the last agent address.
abstract interface class SessionStorage {
  Future<String?> readToken();

  Future<void> writeToken(String token);

  Future<void> deleteToken();

  Future<AgentAddress?> readAgent();

  Future<void> writeAgent(AgentAddress agent);
}

/// Token in `flutter_secure_storage`; host, port and name in
/// `shared_preferences` (not secrets). The token never goes anywhere else.
class DeviceSessionStorage implements SessionStorage {
  DeviceSessionStorage({FlutterSecureStorage? secure})
    : _secure = secure ?? const FlutterSecureStorage();

  static const _tokenKey = 'pairing_token';
  static const _hostKey = 'last_host';
  static const _portKey = 'last_port';
  static const _nameKey = 'last_name';

  final FlutterSecureStorage _secure;

  @override
  Future<String?> readToken() => _secure.read(key: _tokenKey);

  @override
  Future<void> writeToken(String token) =>
      _secure.write(key: _tokenKey, value: token);

  @override
  Future<void> deleteToken() => _secure.delete(key: _tokenKey);

  @override
  Future<AgentAddress?> readAgent() async {
    final prefs = await SharedPreferences.getInstance();
    final host = prefs.getString(_hostKey);
    if (host == null) return null;
    return AgentAddress(
      host: host,
      port: prefs.getInt(_portKey) ?? AgentAddress.defaultPort,
      name: prefs.getString(_nameKey),
    );
  }

  @override
  Future<void> writeAgent(AgentAddress agent) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_hostKey, agent.host);
    await prefs.setInt(_portKey, agent.port);
    final name = agent.name;
    if (name == null) {
      await prefs.remove(_nameKey);
    } else {
      await prefs.setString(_nameKey, name);
    }
  }
}
