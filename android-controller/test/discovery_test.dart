import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:am_remote/core/discovery.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:nsd/nsd.dart';

Uint8List _txt(String value) => Uint8List.fromList(utf8.encode(value));

void main() {
  test('browses the _amremote._tcp service type from the contract', () {
    expect(NsdAgentDiscovery.serviceType, '_amremote._tcp');
  });

  group('agentFromService', () {
    test('reads name and version from TXT and prefers the IPv4 address', () {
      final agent = agentFromService(
        Service(
          name: 'am-remote',
          type: '_amremote._tcp',
          host: 'laptop.local.',
          port: 8765,
          txt: {'version': _txt('1'), 'name': _txt('LAPTOP')},
          addresses: [
            InternetAddress('fe80::1'),
            InternetAddress('192.168.1.20'),
          ],
        ),
      );
      expect(
        agent,
        const DiscoveredAgent(
          name: 'LAPTOP',
          host: '192.168.1.20',
          port: 8765,
          version: '1',
        ),
      );
    });

    test('uses the TXT ip field first, whatever else the resolver gave', () {
      final agent = agentFromService(
        Service(
          name: 'am-remote',
          host: 'fe80::4060:9a09:3f61:5e75%wlan0',
          port: 8765,
          txt: {
            'version': _txt('1'),
            'name': _txt('LAPTOP'),
            'ip': _txt('192.168.1.88'),
          },
          addresses: [InternetAddress('192.168.1.99')],
        ),
      );
      expect(agent?.host, '192.168.1.88');
      expect(agent?.name, 'LAPTOP');
    });

    test('with only an IPv6 host but a TXT ip, the agent is found (the '
        'Android case that left the list empty)', () {
      final agent = agentFromService(
        Service(
          name: 'am-remote',
          host: 'fe80::4060:9a09:3f61:5e75%wlan0',
          port: 8765,
          txt: {'ip': _txt('192.168.1.88')},
        ),
      );
      expect(agent?.host, '192.168.1.88');
      expect(agent?.name, 'am-remote');
    });

    test('without TXT ip it takes an IPv4 address the resolver gave', () {
      final agent = agentFromService(
        Service(
          name: 'am-remote',
          host: 'laptop.local.',
          port: 8765,
          addresses: [InternetAddress('fe80::1'), InternetAddress('10.0.0.5')],
        ),
      );
      expect(agent?.host, '10.0.0.5');
    });

    test('an IPv4 literal in host is used when addresses is empty', () {
      final agent = agentFromService(
        const Service(name: 'am-remote', host: '192.168.1.20', port: 8765),
      );
      expect(agent?.host, '192.168.1.20');
    });

    test('a bad TXT ip is ignored, not trusted', () {
      for (final bad in [
        'laptop.local',
        'fe80::1',
        '192.168.1',
        '0.0.0.0',
        '224.0.0.251',
        '255.255.255.255',
        '192.168.1.1/evil',
        'http://192.168.1.1',
        '192.168.1.1:80',
        '',
      ]) {
        final agent = agentFromService(
          Service(
            name: 'am-remote',
            host: '192.168.1.20',
            port: 8765,
            txt: {'ip': _txt(bad)},
          ),
        );
        expect(agent?.host, '192.168.1.20', reason: 'ip "$bad"');
      }
    });

    test(
      'keeps the resolver\'s other IPv4 addresses as alternatives, in order, '
      'without repeats',
      () {
        final agent = agentFromService(
          Service(
            name: 'am-remote',
            host: 'laptop.local.',
            port: 8765,
            txt: {'ip': _txt('192.168.1.88')},
            addresses: [
              InternetAddress('fe80::1'),
              InternetAddress('192.168.1.88'),
              InternetAddress('10.0.0.5'),
              InternetAddress('192.168.1.99'),
            ],
          ),
        );
        expect(agent?.host, '192.168.1.88');
        expect(agent?.alternativeHosts, ['10.0.0.5', '192.168.1.99']);
        expect(agent?.hosts, ['192.168.1.88', '10.0.0.5', '192.168.1.99']);
      },
    );

    test('a stale TXT ip does not hide the address the resolver gave', () {
      // The laptop moved: the TXT still says .20, the resolver says .99.
      final agent = agentFromService(
        Service(
          name: 'am-remote',
          port: 8765,
          txt: {'ip': _txt('192.168.1.20')},
          addresses: [InternetAddress('192.168.1.99')],
        ),
      );
      expect(agent?.host, '192.168.1.20');
      expect(agent?.alternativeHosts, ['192.168.1.99']);
    });

    test('with one address there are no alternatives', () {
      final agent = agentFromService(
        const Service(name: 'am-remote', host: '192.168.1.20', port: 8765),
      );
      expect(agent?.alternativeHosts, isEmpty);
      expect(agent?.hosts, ['192.168.1.20']);
    });

    test('agents that differ only in their alternatives are not equal', () {
      const a = DiscoveredAgent(name: 'L', host: '10.0.0.1', port: 8765);
      const b = DiscoveredAgent(
        name: 'L',
        host: '10.0.0.1',
        port: 8765,
        alternativeHosts: ['10.0.0.2'],
      );
      expect(a, isNot(b));
      expect(
        b,
        const DiscoveredAgent(
          name: 'L',
          host: '10.0.0.1',
          port: 8765,
          alternativeHosts: ['10.0.0.2'],
        ),
      );
    });

    test('lists an agent whatever its TXT version (it is informational)', () {
      for (final version in ['2', '0', 'abc', '']) {
        final agent = agentFromService(
          Service(
            name: 'am-remote',
            host: '192.168.1.20',
            port: 8765,
            txt: {'version': _txt(version), 'name': _txt('LAPTOP')},
          ),
        );
        expect(agent, isNotNull, reason: 'version "$version"');
        expect(agent?.version, version);
        expect(agent?.name, 'LAPTOP');
      }
    });

    test('lists an agent that has no TXT version at all', () {
      final withOtherTxt = agentFromService(
        Service(
          name: 'am-remote',
          host: '192.168.1.20',
          port: 8765,
          txt: {'name': _txt('LAPTOP')},
        ),
      );
      final withoutTxt = agentFromService(
        const Service(name: 'am-remote', host: '192.168.1.20', port: 8765),
      );
      expect(withOtherTxt, isNotNull);
      expect(withOtherTxt?.version, isNull);
      expect(withoutTxt, isNotNull);
      expect(withoutTxt?.version, isNull);
    });

    test('a host name alone is not a host: the agent is skipped', () {
      expect(
        agentFromService(
          const Service(name: 'am-remote', host: 'laptop.local.', port: 8765),
        ),
        isNull,
      );
    });

    test('skips a service that only resolved to an IPv6 address and has no ip', () {
      // What Android returned on a real run: a link-local address with a zone.
      expect(
        agentFromService(
          const Service(
            name: 'am-remote',
            host: 'fe80::4060:9a09:3f61:5e75%wlan0',
            port: 8765,
          ),
        ),
        isNull,
      );
    });

    test('is null without a port or a host', () {
      expect(agentFromService(const Service(host: 'a.local')), isNull);
      expect(agentFromService(const Service(port: 8765)), isNull);
    });
  });
}
