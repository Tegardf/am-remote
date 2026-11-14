---
name: verify
description: Run the android-controller checks (pub get, build_runner, dart format check, flutter analyze, flutter test) and report pass/fail with the first errors. Use before saying a change is done, before committing, or when the user asks to check the build.
---

# /verify (android-controller)

If there is no `pubspec.yaml` yet, say the app isn't scaffolded (`/build-mvp 0` does that) and stop.
If `flutter` is missing, say so and point to README.md "What to install". Don't install anything.

Run these from this folder, in order, and continue after a failure so the report is complete:
1. `flutter pub get`
2. `dart run build_runner build --delete-conflicting-outputs` (generated `*.g.dart` / `*.freezed.dart` aren't committed)
3. `dart format --output=none --set-exit-if-changed lib test`
4. `flutter analyze`
5. `flutter test`

Report a table with one row per check: check, ✅/❌, one-line summary. Then show the first ~30 lines of each failure.
Don't fix anything unless asked. If asked, fix the code (not by loosening the checks) and re-run.
