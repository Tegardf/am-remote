# MVP 0 — Foundation (desktop-agent)
Follow [_workflow-rules.md](_workflow-rules.md).

## Goal
A tray app that serves the authenticated API skeleton on the LAN, can be found over mDNS, and pairs a phone.

## Gate
- `cargo --version` works, and `rustup show active-toolchain` ends with `pc-windows-msvc`.
- Open question #1 in `../contract/README.md` (no error code for validation) is settled, **or** you use the default below and log a request.

## Build
- [ ] Scaffold if there's no `Cargo.toml`: `cargo init --name am-remote-agent`. Add crates with `cargo add` only when they're first used.
- [ ] `main.rs`: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, tracing setup, then start config → server → mDNS → tray.
- [ ] `config.rs`: port (default 8765), paired devices `{ name, token, paired_at }`, autostart flag. Stored in `%APPDATA%\am-remote\config.json` with an atomic write.
- [ ] `api/mod.rs`: axum router under `/api/v1` on `0.0.0.0:<port>`. A typed `ApiError` → `{ "error": { code, message } }`, also for `JsonRejection`, the 404/405 fallbacks and panics (catch-panic → `INTERNAL`).
- [ ] Validation errors (bad JSON, missing fields): until the contract has a code, return `400` with code `BAD_REQUEST`, kept in one const, and log a request.
- [ ] Auth middleware: `Authorization: Bearer <token>` on everything except `POST /pair`. Also accept `?token=` on `/ws` (used from MVP 2).
- [ ] `api/pair.rs`: 6-digit code from the CSPRNG; `POST /pair` → `{ token }`, where the token is 32 random bytes in base64url. Rotate the code after a successful pair. After 5 wrong codes, lock for 60 s (still `403 BAD_PAIRING_CODE`).
- [ ] `GET /status` → `{ agent_version: CARGO_PKG_VERSION, apple_music_running }`, using the `MediaBackend`.
- [ ] `media/mod.rs`: `trait MediaBackend` plus a `FakeBackend` for tests. The real SMTC backend comes in MVP 1.
- [ ] `discovery.rs`: advertise `_amremote._tcp.local.` on the configured port with TXT `version=1`, `name=<hostname>`.
- [ ] `tray.rs`: the current pairing code, the paired devices each with "Revoke" (drops the token at once), and Quit.

## Tests (required)
- Round-trip the samples `status_response`, `pair_request`, `pair_response` and every `error_*` from `../contract/samples.md`.
- `/pair`: right code → token; wrong code → 403 `BAD_PAIRING_CODE`; 6th wrong attempt → locked; the code rotates after success.
- `/status` without a token → 401 `UNAUTHORIZED` with the error body; with a token → 200.
- Unknown route, wrong method and malformed JSON each return the contract error body (never plain text).
- Config save and load round-trip in a temp dir; a revoked token gets 401.

## Done when
- Step 3 is green (fmt, clippy, test, build).
- `cargo run` shows the tray with a code, and the README's `Invoke-RestMethod` pair → status works.

## Smoke test (→ docs/smoke-tests/mvp-0.md)
1. `cargo run`. The tray icon appears, and the menu shows a 6-digit code.
2. The first run shows a Windows Firewall prompt. Allow it on Private networks.
3. Pair from PowerShell (README "Try the API") → you get a token. `/status` returns `agent_version`.
4. Send a wrong code 6 times → 403 each time, then locked for 60 s.
5. On the phone, use any mDNS/Bonjour browser app (or the app once its MVP 0 exists). `_amremote._tcp` shows the laptop's hostname.
6. Revoke the device in the tray → the same token now gets 401.

## Next
`/build-mvp 1`
