# android-controller

The Flutter Android app: the remote. It finds the desktop agent on your Wi-Fi, pairs with a 6-digit code, and controls Apple Music on the laptop. The phone plays no audio itself.

- Conventions, structure and behaviour rules: [CLAUDE.md](CLAUDE.md)
- API contract (shared with the agent): [../CLAUDE.md](../CLAUDE.md) and [../contract/](../contract/)

> **Status:** not scaffolded yet. `/build-mvp 0` in this window runs `flutter create` and keeps this README, CLAUDE.md and .gitignore.

## What to install

| # | Tool | Why | How |
|---|---|---|---|
| 1 | **Flutter SDK** (stable channel; includes Dart) | build and run the app | https://docs.flutter.dev/get-started/install/windows/mobile → unzip to e.g. `C:\src\flutter` (not under `Program Files`) → add `C:\src\flutter\bin` to your user `PATH` |
| 2 | **Android Studio** | Android SDK, platform-tools (adb), build-tools, emulator, bundled JDK | https://developer.android.com/studio → install → SDK Manager → *SDK Tools* → tick **Android SDK Command-line Tools (latest)** |
| 3 | Android licenses | required by Flutter's Android build | `flutter doctor --android-licenses` → accept all |
| 4 | **Android phone**, Android 7.0+ (API 24) | discovery needs a real device on the same Wi-Fi | Settings → About phone → tap *Build number* 7× → Developer options → **USB debugging** (or **Wireless debugging**) |
| 5 | VS Code extension **Flutter** (installs **Dart**) | editing, run/debug | VS Code Extensions → "Flutter" |
| 6 | Node.js 18+ | Claude Code hook (dart format after each edit) | already installed |

Check:
```powershell
flutter --version
flutter doctor            # ✓ Flutter, ✓ Android toolchain; ✓ Connected device when the phone is plugged in
adb devices               # phone listed as "device" (accept the RSA prompt on the phone)
```

> **adb note:** this laptop already has an `adb` (1.0.41) on `PATH`. If Flutter and that adb disagree ("adb server version doesn't match"), keep only the SDK's `platform-tools` adb on `PATH`.

## Run (after MVP 0)
```powershell
flutter pub get
dart run build_runner build --delete-conflicting-outputs   # generates *.g.dart / *.freezed.dart (not committed)
flutter run                                              # on the connected phone
```
1. Start `desktop-agent` on the laptop first. Phone and laptop must be on the same Wi-Fi.
2. In the app, pick the laptop from the list (or enter its IP), then type the 6-digit code from the agent's tray menu.

**Emulator:** mDNS discovery doesn't reach your LAN from the Android emulator. Use manual IP `10.0.2.2` (the emulator's alias for the host PC) when the agent runs on the same PC.

## Release APK
```powershell
flutter build apk --release
adb install -r build\app\outputs\flutter-apk\app-release.apk
```
The Flutter template signs release builds with the debug key, which is fine for sideloading to your own phone. For a real signing key, create `android/key.properties` plus a keystore. Both are git-ignored and must never be committed.

## Checks
```powershell
flutter pub get
dart run build_runner build --delete-conflicting-outputs
dart format --output=none --set-exit-if-changed lib test
flutter analyze
flutter test
```
Or run `/verify` in Claude Code.

## Claude Code in this window
Open **this folder** (`android-controller/`) in its own VS Code window.

| Command | What it does |
|---|---|
| `/build-mvp [0-3]` | follows `.claude/tasks/mvp-<n>.md`: plan → build in small steps → flutter checks → contract review (reviewer + skeptic subagents) → tick status, log contract requests, write `docs/smoke-tests/mvp-<n>.md` |
| `/verify` | build_runner, format check, analyze, test, and a report |
| `/sync-contract` | apply new `../contract/CHANGELOG.md` entries and `../contract/conformance-report.md` items |

Also in `.claude/`:
- **Tasks:** `tasks/*.md`, step-by-step instructions for each phase, opened only when a command runs. `tasks/_workflow-rules.md` holds the rules every task follows.
- **Rules:** `scope.md` (edit only this folder, Android only, generated code), `api-contract.md` (wire format, models, errors), `security.md` (token storage, logging, cleartext)
- **Hook:** `dart format` after each edit
- **Settings:** flutter/dart commands pre-allowed; edits to `../CLAUDE.md`, `../contract/` and the agent are denied

This window never changes the contract. Gaps go to `docs/contract-requests.md`, and the root window settles them.

## Troubleshooting
| Problem | Fix |
|---|---|
| `flutter` not recognised | Add `<flutter>\bin` to PATH, then open a new terminal or VS Code window |
| `flutter doctor`: Android licenses / cmdline-tools missing | SDK Manager → SDK Tools → Android SDK Command-line Tools, then `flutter doctor --android-licenses` |
| Phone not in `adb devices` | Enable USB debugging, accept the RSA prompt, try another cable/port. See the adb note above |
| App finds no laptop | Same Wi-Fi, laptop network profile Private, agent firewall rule, no guest/AP isolation. Use manual IP |
| `Cleartext HTTP traffic not permitted` | `network_security_config.xml` is missing; MVP 0 adds it |
| Errors about missing `*.g.dart` | Run `dart run build_runner build --delete-conflicting-outputs` |
