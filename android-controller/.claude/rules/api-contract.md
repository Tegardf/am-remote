---
paths:
  - "lib/core/**"
  - "test/**"
  - "build.yaml"
---

# Wire format (app side of the shared contract)

The contract in `../CLAUDE.md` is the source of truth. `../contract/samples.md` has a concrete example for every shape.

- **Field names:** snake_case on the wire. Set it once in `build.yaml` (`json_serializable` → `field_rename: snake`) rather than per field, and use `@JsonKey(name: …)` only where a name doesn't follow the rule.
- **Types:** times are `int` milliseconds and volume is an `int` 0-100. Convert to `Duration` / `double` in the UI layer, not in the models.
- **Nullable:** `playlist_name` is `String?`. Present-but-null is normal.
- **Status enum:** `playing|paused|stopped|unknown`, and any unrecognised value maps to `unknown` (`@JsonKey(unknownEnumValue: PlaybackStatus.unknown)`).
- **Be forward compatible:** ignore unknown JSON fields (don't use `disallowUnrecognizedKeys`) and unknown WebSocket `type`s.
- **204 has no body.** Don't try to decode one.
- **Errors:** turn every non-2xx `{ "error": { code, message } }` into a typed `ApiException(code, message, httpStatus)` in `api_client.dart`. UI logic switches on `code` (`UNAUTHORIZED`, `BAD_PAIRING_CODE`, `APPLE_MUSIC_NOT_RUNNING`, `NOT_SUPPORTED`, `INTERNAL`), never on the message text. Unknown codes are treated like `INTERNAL`.
- **WebSocket:** messages are `{ "type", "data" }`. Send `{ "type": "ping" }` every 15 s, and treat a missing `pong` as a dead socket (reconnect with backoff).
- **Tests:** `test/contract_samples_test.dart` reads `File('../contract/samples.md')` (`flutter test` runs from the package root) and splits it into samples: each `### <key>` heading is followed by one fenced `json` block. Round-trip every sample this side models (`fromJson` → `toJson` equals the sample). When you add a model, add its key to the test.
- **Contract wrong or incomplete?** Don't change the wire shape here. Log it in `docs/contract-requests.md` (see `scope.md`).
