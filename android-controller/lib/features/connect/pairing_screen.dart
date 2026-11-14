import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:material_ui/material_ui.dart';

import '../../core/agent_address.dart';
import '../../core/api_client.dart';
import '../../core/providers.dart';
import 'session_controller.dart';

/// Asks for the 6-digit code shown in the agent's tray menu.
class PairingScreen extends ConsumerStatefulWidget {
  const PairingScreen({super.key, required this.agent});

  final AgentAddress agent;

  @override
  ConsumerState<PairingScreen> createState() => _PairingScreenState();
}

class _PairingScreenState extends ConsumerState<PairingScreen> {
  final _formKey = GlobalKey<FormState>();
  final _codeController = TextEditingController();
  final _nameController = TextEditingController();
  bool _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    ref.read(deviceNameProvider.future).then((name) {
      if (mounted && _nameController.text.isEmpty) _nameController.text = name;
    });
  }

  @override
  void dispose() {
    _codeController.dispose();
    _nameController.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    if (_busy || !_formKey.currentState!.validate()) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    final navigator = Navigator.of(context);
    final sessionController = ref.read(sessionControllerProvider.notifier);
    try {
      final client = ref.read(apiClientFactoryProvider)(widget.agent);
      final response = await client.pair(
        code: _codeController.text,
        deviceName: _nameController.text.trim(),
      );
      await sessionController.completePairing(widget.agent, response.token);
      if (mounted) setState(() => _busy = false);
      // The session switched to the app; drop the connect flow on top of it.
      navigator.popUntil((route) => route.isFirst);
    } on ApiException catch (e) {
      _fail(
        e.code == ApiErrorCode.badPairingCode
            ? 'Wrong or expired code'
            : 'The laptop reported an error. Please try again.',
      );
    } on AgentUnreachableException {
      _fail("Can't reach ${widget.agent.hostPort}. Same Wi-Fi?");
    } catch (e) {
      // Debug builds only; the exception type and host hold no secrets.
      if (kDebugMode) {
        debugPrint(
          'Pairing failed for ${widget.agent.hostPort}: ${e.runtimeType}: $e',
        );
      }
      _fail('Something went wrong. Please try again.');
    }
  }

  void _fail(String message) {
    if (!mounted) return;
    setState(() {
      _busy = false;
      _error = message;
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text('Pair with ${widget.agent.displayName}')),
      body: SafeArea(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(24),
          child: Form(
            key: _formKey,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  "Open the agent's tray menu on your laptop and type the "
                  '6-digit pairing code.',
                  style: theme.textTheme.bodyLarge,
                ),
                const SizedBox(height: 24),
                TextFormField(
                  controller: _codeController,
                  enabled: !_busy,
                  autofocus: true,
                  keyboardType: TextInputType.number,
                  maxLength: 6,
                  inputFormatters: [FilteringTextInputFormatter.digitsOnly],
                  style: theme.textTheme.headlineMedium?.copyWith(
                    letterSpacing: 8,
                  ),
                  textAlign: TextAlign.center,
                  decoration: const InputDecoration(
                    labelText: 'Pairing code',
                    counterText: '',
                  ),
                  validator: (value) =>
                      (value ?? '').length == 6 ? null : 'Enter all 6 digits',
                  onFieldSubmitted: (_) => _submit(),
                ),
                const SizedBox(height: 16),
                TextFormField(
                  controller: _nameController,
                  enabled: !_busy,
                  textInputAction: TextInputAction.done,
                  decoration: const InputDecoration(
                    labelText: 'Name of this phone',
                  ),
                  validator: (value) => (value ?? '').trim().isEmpty
                      ? 'Enter a name for this phone'
                      : null,
                  onFieldSubmitted: (_) => _submit(),
                ),
                if (_error != null) ...[
                  const SizedBox(height: 16),
                  Text(
                    _error!,
                    textAlign: TextAlign.center,
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
                ],
                const SizedBox(height: 24),
                FilledButton(
                  onPressed: _busy ? null : _submit,
                  child: _busy
                      ? const SizedBox.square(
                          dimension: 20,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Text('Pair'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
