import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/foundation.dart' show listEquals;
import 'package:nsd/nsd.dart' as nsd;

import 'agent_address.dart';

/// An agent found on the LAN via mDNS (`_amremote._tcp`).
class DiscoveredAgent {
  const DiscoveredAgent({
    required this.name,
    required this.host,
    required this.port,
    this.version,
    this.alternativeHosts = const [],
  });

  /// TXT `name` (the laptop's hostname); falls back to the mDNS service name.
  final String name;

  /// The address to use: the TXT `ip`, else an IPv4 address from the resolver.
  final String host;
  final int port;

  /// TXT `version` (contract version, "1"), if present.
  final String? version;

  /// Other IPv4 addresses the resolver gave. The TXT `ip` can be out of date
  /// after a DHCP renewal or a Wi-Fi switch (contract C-004), so when [host]
  /// can't be reached these are tried next.
  final List<String> alternativeHosts;

  /// [host] first, then the [alternativeHosts].
  List<String> get hosts => [host, ...alternativeHosts];

  AgentAddress toAddress() => AgentAddress(host: host, port: port, name: name);

  @override
  bool operator ==(Object other) =>
      other is DiscoveredAgent &&
      other.name == name &&
      other.host == host &&
      other.port == port &&
      other.version == version &&
      listEquals(other.alternativeHosts, alternativeHosts);

  @override
  int get hashCode =>
      Object.hash(name, host, port, version, Object.hashAll(alternativeHosts));
}

/// Finds agents on the LAN. Manual host:port entry is the fallback and lives
/// in the connect screen.
abstract interface class AgentDiscovery {
  /// Emits each agent as it is found, and completes after [timeout].
  Stream<DiscoveredAgent> discover({
    Duration timeout = const Duration(seconds: 8),
  });
}

class NsdAgentDiscovery implements AgentDiscovery {
  const NsdAgentDiscovery();

  static const serviceType = '_amremote._tcp';

  @override
  Stream<DiscoveredAgent> discover({
    Duration timeout = const Duration(seconds: 8),
  }) {
    late final StreamController<DiscoveredAgent> controller;
    nsd.Discovery? discovery;
    Timer? timer;
    var closed = false;

    Future<void> stop() async {
      if (closed) return;
      closed = true;
      timer?.cancel();
      final d = discovery;
      if (d != null) {
        try {
          await nsd.stopDiscovery(d);
        } catch (_) {
          // Already stopped: nothing to free.
        }
      }
      if (!controller.isClosed) await controller.close();
    }

    controller = StreamController<DiscoveredAgent>(
      onListen: () async {
        timer = Timer(timeout, stop);
        try {
          discovery = await nsd.startDiscovery(
            serviceType,
            ipLookupType: nsd.IpLookupType.v4,
          );
          if (closed) {
            await stop();
            return;
          }
          discovery!.addServiceListener((service, status) {
            if (status != nsd.ServiceStatus.found || closed) return;
            final agent = agentFromService(service);
            if (agent != null) controller.add(agent);
          });
          // Services found before the listener was attached.
          for (final service in discovery!.services) {
            final agent = agentFromService(service);
            if (agent != null) controller.add(agent);
          }
        } catch (e, st) {
          if (!controller.isClosed) controller.addError(e, st);
          await stop();
        }
      },
      onCancel: stop,
    );
    return controller.stream;
  }
}

/// Turns a resolved mDNS service into an agent. The host is, in this order
/// (contract C-002):
///
/// 1. the TXT field `ip` (the laptop's LAN IPv4 address),
/// 2. an IPv4 address the resolver gave,
/// 3. nothing: the agent is skipped.
///
/// The other IPv4 addresses are kept as [DiscoveredAgent.alternativeHosts] in
/// the same order, for when the TXT `ip` turns out to be stale (C-004).
///
/// The agent listens on IPv4 only, and Android often resolves only an IPv6
/// link-local address (`fe80::…%wlan0`) or a `.local` name it can't connect to,
/// so neither is ever used as the host. The user can still enter the IPv4
/// address manually. Returns null when there is no usable host or port.
DiscoveredAgent? agentFromService(nsd.Service service) {
  final port = service.port;
  if (port == null) return null;

  final hosts = _hostsFor(service);
  if (hosts.isEmpty) return null;
  final host = hosts.first;

  final txtName = _txtString(service.txt?['name']);
  final name = (txtName != null && txtName.isNotEmpty)
      ? txtName
      : (service.name ?? host);
  return DiscoveredAgent(
    name: name,
    host: host,
    port: port,
    version: _txtString(service.txt?['version']),
    alternativeHosts: hosts.skip(1).toList(),
  );
}

/// Every usable IPv4 address of the service, best first and without repeats:
/// the TXT `ip`, then the resolver's addresses, then an IPv4 literal in `host`
/// (some Android versions leave `addresses` empty and put the IP there).
List<String> _hostsFor(nsd.Service service) {
  final found = <String>[];
  void add(String? candidate) {
    final usable = _ipv4Literal(candidate);
    if (usable != null && !found.contains(usable)) found.add(usable);
  }

  add(_txtString(service.txt?['ip']));
  for (final address in service.addresses ?? const <InternetAddress>[]) {
    if (address.type == InternetAddressType.IPv4) add(address.address);
  }
  add(service.host);
  return found;
}

/// [text] if it is a plain dotted IPv4 address the app can connect to, else
/// null. Anything else (a name, IPv6, `0.0.0.0`, multicast) is not a host.
String? _ipv4Literal(String? text) {
  final candidate = text?.trim();
  if (candidate == null || candidate.isEmpty) return null;
  final parts = candidate.split('.');
  if (parts.length != 4 ||
      parts.any((p) => p.isEmpty || p.length > 3 || int.tryParse(p) == null)) {
    return null;
  }
  final address = InternetAddress.tryParse(candidate);
  if (address == null ||
      address.type != InternetAddressType.IPv4 ||
      address.isMulticast ||
      candidate == '0.0.0.0' ||
      candidate == '255.255.255.255') {
    return null;
  }
  return candidate;
}

String? _txtString(Uint8List? bytes) {
  if (bytes == null) return null;
  try {
    return utf8.decode(bytes);
  } catch (_) {
    return null;
  }
}
