# Working in the android-controller window

- **Edit only inside this folder.** `../CLAUDE.md` (the roadmap and API contract) and `../contract/` belong to the root window. Read them, but never edit them. `../desktop-agent/` is a different project.
- **Contract problems go to a request file, not into the contract.** If the contract is silent, contradictory, or missing something you need, pick the least surprising behaviour and keep it in one place. Then append it to `docs/contract-requests.md` in this format:
  `- [ ] <what the contract should say> - why: <reason> - default used: <what the code does now> (MVP <n>, YYYY-MM-DD)`
  The root window settles these with `/contract-change`, and this window applies the result with `/sync-contract`.
- **Android only.** Don't add iOS, web or desktop platforms. Keep the package `am_remote` and org `dev.amremote`.
- **Generated code isn't committed.** Run `dart run build_runner build --delete-conflicting-outputs` after changing models, and never hand-edit `*.g.dart` or `*.freezed.dart`.
- **Done means `/verify` is green:** build_runner, `dart format` check, `flutter analyze` and `flutter test`. Tests must not need a running agent, the network, or a device; use a fake ApiClient and a fake WebSocket stream.
- **Device-only behaviour** (discovery on real Wi-Fi, media notification, lock screen, volume keys): list the manual check in the phase's smoke test (`docs/smoke-tests/`). Don't claim it works untested.
- **Status:** tick `- [x] MVP <n>` in ./CLAUDE.md only when the whole phase works and checks pass. Keep "Contract changes applied:" up to date (see `/sync-contract`).
- **Commits** (only when the user asks): `feat(app): …`, `fix(app): …`, `test(app): …`, `docs(app): …`. Never commit keystores, `android/key.properties` or `android/local.properties`.
