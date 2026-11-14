---
paths:
  - "src/api/**"
  - "src/media/**"
  - "src/discovery.rs"
  - "tests/**"
---

# Wire format (agent side of the shared contract)

The contract in `../CLAUDE.md` is the source of truth. `../contract/samples.md` has a concrete example for every shape.

- **Field names:** snake_case exactly as in the contract. Put `#[serde(rename_all = "snake_case")]` on every wire struct, and lowercase variants on the status enum (`playing|paused|stopped|unknown`).
- **Types:** times are integer milliseconds (`u64`) and volume is an integer `0..=100` (`u8`); never floats on the wire. Convert at the edge: Core Audio's 0.0-1.0 and the WinRT TimeSpan's 100 ns ticks never leave the media module.
- **Always present:** `playlist_name` is serialized as `null` when unknown, never omitted. No `skip_serializing_if` on contract fields.
- **Success with no body** = `204` and an empty body. Don't return `{}` or `"ok"`.
- **Errors:** every non-2xx goes through one `ApiError` → `{ "error": { "code", "message" } }`, using only the contract's codes and HTTP statuses. That includes axum `JsonRejection` / `QueryRejection`, the 404/405 fallbacks, and panics (catch-panic layer → `INTERNAL`). Messages are for humans; the app switches on `code`.
- **Request bodies:** ignore unknown fields (serde's default). Don't use `deny_unknown_fields`; newer apps may send more.
- **WebSocket:** every server message is `{ "type": "<event>", "data": { … } }`. Send `state` on connect and on every change. Answer `{ "type": "ping" }` with `{ "type": "pong" }`, and ignore unknown client message types.
- **Discovery:** service `_amremote._tcp.local.`, the configured port, TXT `version=1` and `name=<hostname>`.
- **Tests:** `tests/contract_samples.rs` reads `../contract/samples.md` (`include_str!("../../contract/samples.md")`) and splits it into samples: each `### <key>` heading is followed by one fenced `json` block. Round-trip every sample this side models: deserialize into the real type, serialize again, and assert the JSON is equal. When you add a type, add its key to the test.
- **Contract wrong or incomplete?** Don't change the wire shape here. Log it in `docs/contract-requests.md` (see `scope.md`).
