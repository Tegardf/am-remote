import 'package:freezed_annotation/freezed_annotation.dart';

import 'player_state.dart';
import 'volume.dart';

part 'ws_message.freezed.dart';
part 'ws_message.g.dart';

/// One WebSocket frame: `{ "type": "<event>", "data": { … } }`.
/// `ping` and `pong` have no `data`, so a null `data` is not written.
@freezed
abstract class WsMessage with _$WsMessage {
  // Freezed copies these options onto the generated class.
  // ignore: invalid_annotation_target
  @JsonSerializable(includeIfNull: false)
  const factory WsMessage({required String type, Map<String, Object?>? data}) =
      _WsMessage;

  factory WsMessage.fromJson(Map<String, Object?> json) =>
      _$WsMessageFromJson(json);
}

/// What the app understands from the agent. Unknown types become null in
/// [parseWsEvent] and are ignored.
sealed class WsEvent {
  const WsEvent();
}

class StateEvent extends WsEvent {
  const StateEvent(this.state);
  final PlayerState state;
}

class VolumeEvent extends WsEvent {
  const VolumeEvent(this.volume);
  final Volume volume;
}

class AppleMusicEvent extends WsEvent {
  const AppleMusicEvent({required this.running});
  final bool running;
}

class PongEvent extends WsEvent {
  const PongEvent();
}

/// Turns a frame into an event, or null for an unknown type or a malformed
/// payload: one bad message must never take the connection down.
WsEvent? parseWsEvent(WsMessage message) {
  final data = message.data;
  try {
    switch (message.type) {
      case 'state':
        return data == null ? null : StateEvent(PlayerState.fromJson(data));
      case 'volume':
        return data == null ? null : VolumeEvent(Volume.fromJson(data));
      case 'apple_music':
        final running = data?['running'];
        return running is bool ? AppleMusicEvent(running: running) : null;
      case 'pong':
        return const PongEvent();
    }
  } catch (_) {
    return null;
  }
  return null;
}
