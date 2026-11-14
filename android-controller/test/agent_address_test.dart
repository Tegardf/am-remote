import 'package:am_remote/core/agent_address.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AgentAddress.parse', () {
    test('uses the default port when none is given', () {
      final a = AgentAddress.parse('192.168.1.5')!;
      expect((a.host, a.port), ('192.168.1.5', 8765));
    });

    test('reads host and port, ignoring surrounding spaces', () {
      final a = AgentAddress.parse('  my-laptop.local:9000 ')!;
      expect((a.host, a.port), ('my-laptop.local', 9000));
    });

    for (final bad in [
      '',
      '   ',
      'http://192.168.1.5',
      '192.168.1.5/path',
      '192.168.1.5:0',
      '192.168.1.5:70000',
      '192.168.1.5:abc',
      '1.2.3.4:80:90',
      'evil.com@192.168.1.5',
      '-bad',
    ]) {
      test('rejects "$bad"', () => expect(AgentAddress.parse(bad), isNull));
    }
  });
}
