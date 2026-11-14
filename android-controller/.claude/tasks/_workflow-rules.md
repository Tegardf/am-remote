# Rules for running a task (android-controller)

Read this before any task in `.claude/tasks/`. It isn't loaded at session start; `/build-mvp` and `/sync-contract` open it when they run.

## Ownership
- Edit only inside `android-controller/`. `../CLAUDE.md` and `../contract/` are read-only from here. `../desktop-agent/` is another project.
- Never install system software. If a tool is missing, stop and point to README.md "What to install".
- Never commit. At the end, suggest a message (`feat(app): …`, `fix(app): …`, `docs(app): …`).

## The five steps of an MVP task
Track them with the todo list, and say which step you're on.

### 1. Plan (read only)
Read the task file, ./CLAUDE.md ("Current status" and "Contract changes applied:"), `../contract/CHANGELOG.md`, `../contract/conformance-report.md` (if present), `docs/contract-requests.md` (if present) and the existing code.
Build the todo list in this order:
1. Unfinished items from earlier MVPs (unticked in "Current status")
2. CHANGELOG entries newer than "Contract changes applied" that apply to `app`
3. Open `android-controller` / `both` items in the conformance report
4. This task's **Build** checklist

Skip anything that already exists and works. Show the list to the user, then continue.

**Stop** and report, without coding, if:
- the task's **Gate** isn't met
- `flutter doctor` shows the Flutter or Android toolchain broken
- a contract question has no safe default. Log it in `docs/contract-requests.md` and tell the user to run `/contract-change` in the root window.

### 2. Build
- Work in small steps: at most 3 todo items, then run `dart run build_runner build --delete-conflicting-outputs` (if models changed) and `flutter analyze`, and fix what you broke before moving on.
- Write the tests for a piece of code in the same step as the code. No test may need a running agent, the network, or a device; use a fake ApiClient and a fake WebSocket stream. Screens get widget tests.
- **Contract gaps** (the contract is silent or unclear): choose the least surprising behaviour, keep it in one place, and log it in `docs/contract-requests.md`:
  `- [ ] <what the contract should say> - why: <reason> - default used: <what the code does> (MVP <n>, YYYY-MM-DD)`
- **Device-only behaviour** (Wi-Fi discovery, media notification, lock screen, volume keys) goes in the smoke test. Don't claim it works untested.
- Android only. Package `am_remote`, org `dev.amremote`. Never hand-edit `*.g.dart` or `*.freezed.dart`.

### 3. Check
Run, in order: `flutter pub get` → `dart run build_runner build --delete-conflicting-outputs` → `dart format lib test` → `flutter analyze` → `flutter test`.
Fix the code, not the checks: don't disable lints globally, and don't skip or delete tests.
If you're stuck on the same failure after two real attempts, stop and report it with the error text.

### 4. Contract review (independent eyes)
1. Spawn a **read-only** `general-purpose` subagent with the *Review prompt* below, filled in with the MVP number.
2. If it returns findings, spawn a second **read-only** subagent with the *Skeptic prompt* and the findings.
3. Fix only the confirmed mismatches, each with a test that would have caught it. Then repeat step 3.
4. Findings of kind `contract-gap` aren't fixed here. Log them in `docs/contract-requests.md`.

### 5. Wrap-up
- If the whole task is done and step 3 is green, tick `- [x] MVP <n>` in ./CLAUDE.md.
- Set "Contract changes applied:" to the newest CHANGELOG id you implemented.
- Write `docs/smoke-tests/mvp-<n>.md` (overwrite) from the task's **Smoke test** section. Use numbered steps with expected results, plus everything that needs a human.
- Report in the format at the end of this file.

## Review prompt
```
Read-only review: do not edit any file. You are in android-controller/.
Compare the app's API usage with the API contract in ../CLAUDE.md ("API contract (v1)") and the examples
in ../contract/samples.md. Required: every contract item whose MVP column is <= <N>; later items only if
implemented (then they must be right).
Check: base URL http://<host>:8765/api/v1, paths and methods; Bearer header on every call except POST /pair;
WS URL /ws?token=; JSON names after json_serializable renames / build.yaml (snake_case), int ms, volume
0-100, nullable playlist_name; unknown status -> unknown; unknown fields and WS types ignored; 204 not
decoded; error JSON -> typed exception keyed on code (UNAUTHORIZED -> clear token + pairing,
APPLE_MUSIC_NOT_RUNNING -> empty state, NOT_SUPPORTED -> explanation); ping every 15 s; discovery browses
_amremote._tcp and reads TXT version/name; a test round-trips the samples for every model.
Report findings as: kind (mismatch | missing-test | contract-gap), file:line, contract item, expected, actual
(quote the code). Behaviour that matches is not a finding.
```

## Skeptic prompt
```
Read-only: do not edit any file. You are in android-controller/. Try to refute each finding below by
opening the cited code and ../CLAUDE.md yourself. Common false alarms: build.yaml field_rename already
produces snake_case; errors are mapped centrally in api_client; the item belongs to a later MVP and isn't
implemented; the test exists under another name. For each finding answer: CONFIRMED or REJECTED, with one
line of evidence. Default to REJECTED when you cannot confirm it.
<findings>
```

## Report format
```
MVP <n> (android-controller): done | partial | blocked
Done:        <items>
Left:        <items + reason>
Checks:      build_runner ✅ format ✅ analyze ✅ test ✅
Contract:    <confirmed>/<found> fixed · <rejected> false alarms · requests logged: <n>
Needs you:   <manual checks on the phone + laptop>
Smoke test:  docs/smoke-tests/mvp-<n>.md
Next:        <command>
Commit:      feat(app): MVP <n> - <summary>
```
