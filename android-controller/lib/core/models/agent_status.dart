import 'package:freezed_annotation/freezed_annotation.dart';

part 'agent_status.freezed.dart';
part 'agent_status.g.dart';

/// `GET /status` response.
@freezed
abstract class AgentStatus with _$AgentStatus {
  const factory AgentStatus({
    required String agentVersion,
    required bool appleMusicRunning,
  }) = _AgentStatus;

  factory AgentStatus.fromJson(Map<String, Object?> json) =>
      _$AgentStatusFromJson(json);
}
