import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/agent_address.dart';
import 'discovery_controller.dart';
import 'pairing_screen.dart';

/// Shown while there is no valid token: pick a laptop, then pair with it.
class ConnectScreen extends ConsumerWidget {
  const ConnectScreen({super.key});

  void _openPairing(BuildContext context, AgentAddress agent) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(builder: (_) => PairingScreen(agent: agent)),
    );
  }

  Future<void> _enterManually(BuildContext context) async {
    final agent = await showDialog<AgentAddress>(
      context: context,
      builder: (_) => const _ManualAddressDialog(),
    );
    if (agent != null && context.mounted) _openPairing(context, agent);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final discovery = ref.watch(discoveryControllerProvider);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Connect to your laptop')),
      body: Column(
        children: [
          if (discovery.searching) const LinearProgressIndicator(),
          Expanded(
            child: discovery.agents.isEmpty
                ? Center(
                    child: Padding(
                      padding: const EdgeInsets.all(24),
                      child: Text(
                        discovery.searching
                            ? 'Looking for your laptop on this Wi-Fi…'
                            : discovery.failed
                            ? 'Discovery failed. Check your Wi-Fi, or enter the '
                                  "laptop's IP address."
                            : 'No laptop found. Make sure the agent is running '
                                  'and both devices are on the same Wi-Fi.',
                        textAlign: TextAlign.center,
                        style: theme.textTheme.bodyLarge,
                      ),
                    ),
                  )
                : ListView(
                    children: [
                      for (final agent in discovery.agents)
                        ListTile(
                          leading: const Icon(Icons.laptop),
                          title: Text(agent.name),
                          subtitle: Text('${agent.host}:${agent.port}'),
                          trailing: const Icon(Icons.chevron_right),
                          onTap: () => _openPairing(context, agent.toAddress()),
                        ),
                    ],
                  ),
          ),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  Expanded(
                    child: OutlinedButton.icon(
                      onPressed: discovery.searching
                          ? null
                          : () => ref
                                .read(discoveryControllerProvider.notifier)
                                .search(),
                      icon: const Icon(Icons.refresh),
                      label: const Text('Search again'),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: FilledButton.icon(
                      onPressed: () => _enterManually(context),
                      icon: const Icon(Icons.edit),
                      label: const Text('Enter IP manually'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ManualAddressDialog extends StatefulWidget {
  const _ManualAddressDialog();

  @override
  State<_ManualAddressDialog> createState() => _ManualAddressDialogState();
}

class _ManualAddressDialogState extends State<_ManualAddressDialog> {
  final _controller = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() {
    final agent = AgentAddress.parse(_controller.text);
    if (agent == null) {
      setState(
        () =>
            _error = 'Enter an address like 192.168.1.20 or 192.168.1.20:8765',
      );
      return;
    }
    Navigator.of(context).pop(agent);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Laptop address'),
      content: TextField(
        controller: _controller,
        autofocus: true,
        keyboardType: TextInputType.url,
        decoration: InputDecoration(
          labelText: 'IP address',
          hintText: '192.168.1.20',
          helperText:
              'Port ${AgentAddress.defaultPort} is used unless you add one',
          errorText: _error,
        ),
        onSubmitted: (_) => _submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Continue')),
      ],
    );
  }
}
