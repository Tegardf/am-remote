# MVP 0 — Connect & pair (android-controller)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
The app finds the agent on the Wi-Fi (or you type its IP), pairs with the 6-digit code, keeps the token, and shows whether it's connected.

## Gate
- `flutter doctor`: Flutter ✓, Android toolchain ✓ (licenses accepted).

## Build
- [ ] Scaffold if there's no `pubspec.yaml`: `flutter create --org dev.amremote --project-name am_remote --platforms android .`. It keeps the existing README.md, CLAUDE.md and .gitignore; delete the counter demo.
- [ ] Packages (`flutter pub add`): `flutter_riverpod`, `dio`, `nsd`, `flutter_secure_storage`, `shared_preferences`, `freezed_annotation`, `json_annotation`. Dev: `build_runner`, `freezed`, `json_serializable`. Add others only when a later MVP needs them.
- [ ] `build.yaml`: `json_serializable` → `field_rename: snake`.
- [ ] Android: `minSdk 24`; manifest permissions `INTERNET`, `ACCESS_NETWORK_STATE`, `CHANGE_WIFI_MULTICAST_STATE`; `android:allowBackup="false"`; `res/xml/network_security_config.xml` allowing cleartext, referenced from the manifest.
- [ ] `core/models/`: `AgentStatus`, `PairRequest`, `PairResponse`, `ApiErrorBody` (freezed + json).
- [ ] `core/api_client.dart`: dio with base URL `http://<host>:<port>/api/v1` and a bearer interceptor (not on `/pair`). Non-2xx → `ApiException(code, message, status)`; 204 isn't decoded. A provider exposes it.
- [ ] `core/discovery.dart`: browse `_amremote._tcp` with `nsd`, yielding `{ name (TXT name), host, port, version }`, with a timeout. Keep a manual host:port fallback.
- [ ] `features/connect/`: discovered-agents list, "Enter IP manually", and a pairing screen (6 digits + device name, defaulting to the phone model). Show `BAD_PAIRING_CODE` as "Wrong or expired code".
- [ ] Token in `flutter_secure_storage`; last host in `shared_preferences`. On start: saved host + token → `GET /status` → home; `401` → clear the token → pairing.
- [ ] Connection indicator (connected / connecting / offline) in the app bar. Home is a placeholder Player screen.

## Tests (required)
- Round-trip `status_response`, `pair_request`, `pair_response` and every `error_*` from `../contract/samples.md` (read the file, extract each `### key` + `json` block).
- `api_client`: a 401 body → `ApiException('UNAUTHORIZED')`; an unknown code → treated as INTERNAL; 204 with no body → OK.
- Pairing screen widget test: rejects non-6-digit input, and shows the error for `BAD_PAIRING_CODE`.
- Startup flow with a fake client: token + 200 → home; 401 → pairing, with the token cleared.

## Done when
- Step 3 is green.
- On a real phone, the app finds the laptop and pairs with the agent (needs a human, and the agent's MVP 0).

## Smoke test (→ docs/smoke-tests/mvp-0.md)
1. Agent running on the laptop; phone on the same Wi-Fi. `flutter run`.
2. The laptop's hostname appears in the list within ~5 s. Tap it, then enter the tray code → home, indicator "connected".
3. Kill and reopen the app → it goes straight home (token kept).
4. Revoke the phone in the agent's tray, then reopen → back to pairing.
5. Enter a wrong code → "Wrong or expired code".
6. Use "Enter IP manually" with the laptop's IP → pairing works without discovery.
7. Turn Wi-Fi off → indicator "offline"; Wi-Fi on → "connected".

## Next
`/build-mvp 1`
