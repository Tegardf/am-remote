/// Where the desktop agent lives on the LAN.
///
/// The app only ever sends HTTP to an address the user picked or typed, so
/// [parse] accepts nothing but a plain host name / IPv4 and an optional port.
class AgentAddress {
  const AgentAddress({required this.host, this.port = defaultPort, this.name});

  static const int defaultPort = 8765;

  final String host;
  final int port;

  /// Friendly name (the agent's hostname from mDNS TXT `name`), display only.
  final String? name;

  String get hostPort => '$host:$port';

  /// `ws://<host>:<port>/api/v1/ws?token=<token>`. The URL carries the token:
  /// never log it.
  Uri webSocketUri(String token) => Uri(
    scheme: 'ws',
    host: host,
    port: port,
    path: '/api/v1/ws',
    queryParameters: {'token': token},
  );

  String get displayName => (name != null && name!.isNotEmpty) ? name! : host;

  static final RegExp _hostPattern = RegExp(
    r'^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$',
  );

  /// Parses `192.168.1.5`, `192.168.1.5:8765` or `my-laptop.local:8765`.
  /// Returns null when the text isn't a usable host[:port].
  static AgentAddress? parse(String input) {
    final text = input.trim();
    if (text.isEmpty) return null;
    final parts = text.split(':');
    if (parts.length > 2) return null;
    final host = parts[0];
    if (!_hostPattern.hasMatch(host)) return null;
    var port = defaultPort;
    if (parts.length == 2) {
      final parsed = int.tryParse(parts[1]);
      if (parsed == null || parsed < 1 || parsed > 65535) return null;
      port = parsed;
    }
    return AgentAddress(host: host, port: port);
  }

  @override
  bool operator ==(Object other) =>
      other is AgentAddress &&
      other.host == host &&
      other.port == port &&
      other.name == name;

  @override
  int get hashCode => Object.hash(host, port, name);

  @override
  String toString() => 'AgentAddress($hostPort)';
}
