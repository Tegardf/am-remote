# Rules for running a task (desktop-agent)

Read this before any task in `.claude/tasks/`. It isn't loaded at session start; `/build-mvp`, `/spike` and `/sync-contract` open it when they run.

## Ownership
- Edit only inside `desktop-agent/`. `../CLAUDE.md` and `../contract/` are read-only from here. `../android-controller/` is another project.
- Never install system software. If a tool is missing, stop and point to README.md "What to install".
- Never commit. At the end, suggest a message (`feat(agent): …`, `fix(agent): …`, `docs(agent): …`).

## The five steps of an MVP task
Track them with the todo list, and say which step you're on.

### 1. Plan (read only)
Read the task file, ./CLAUDE.md ("Current status" and "Contract changes applied:"), `../contract/CHANGELOG.md`, `../contract/conformance-report.md` (if present), `docs/contract-requests.md` (if present) and the existing code.
Build the todo list in this order:
1. Unfinished items from earlier MVPs (unticked in "Current status")
2. CHANGELOG entries newer than "Contract changes applied" that apply to `agent`
3. Open `desktop-agent` / `both` items in the conformance report
4. This task's **Build** checklist

Skip anything that already exists and works. Show the list to the user, then continue.

**Stop** and report, without coding, if:
- the task's **Gate** isn't met
- the toolchain is missing
- a contract question has no safe default. Log it in `docs/contract-requests.md` and tell the user to run `/contract-change` in the root window.

### 2. Build
- Work in small steps: at most 3 todo items, then run `cargo check --all-targets`, and fix what you broke before moving on.
- Write the tests for a piece of code in the same step as the code. No test may need Apple Music, the tray, a window, or the network beyond localhost; use the fake `MediaBackend`.
- **Contract gaps** (the contract is silent or unclear): choose the least surprising behaviour, keep it in one const or function, and log it in `docs/contract-requests.md`:
  `- [ ] <what the contract should say> - why: <reason> - default used: <what the code does> (MVP <n>, YYYY-MM-DD)`
- **Real-machine facts** (AUMID, process name, UIA selectors): probe them with a small `examples/` program, and keep each in one const commented `// verified: <how, date>` or `// UNVERIFIED`.
- `cargo run` starts a server and tray. Use it only for short smoke checks, and stop it afterwards.

### 3. Check
Run, in order: `cargo fmt --all` → `cargo clippy --all-targets -- -D warnings` → `cargo test` → `cargo build`.
Fix the code, not the checks: no blanket `#[allow]`, and no `#[ignore]`d or deleted tests. A narrowly scoped `allow` with a comment is fine for a real false positive.
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
Read-only review: do not edit any file. You are in desktop-agent/.
Compare the agent's wire behaviour with the API contract in ../CLAUDE.md ("API contract (v1)")
and the examples in ../contract/samples.md. Required: every contract item whose MVP column is <= <N>;
later items only if implemented (then they must be right).
Check: routes (method, path under /api/v1, port 8765); 204 with empty body vs 200 JSON; Bearer auth on
everything except POST /pair, WS auth via ?token=; serde names after rename attributes (snake_case),
integer ms, volume 0-100, playlist_name present as null; status enum spelling; every non-2xx incl. axum
rejections, 404/405 fallbacks and panics uses {"error":{"code","message"}} with only the listed codes and
statuses; WS envelope {type,data}, state on connect and on change, ping -> pong; mDNS _amremote._tcp.local.,
TXT version=1 and name; a test round-trips the samples for every shape implemented.
Report findings as: kind (mismatch | missing-test | contract-gap), file:line, contract item, expected, actual
(quote the code). Behaviour that matches is not a finding.
```

## Skeptic prompt
```
Read-only: do not edit any file. You are in desktop-agent/. Try to refute each finding below by opening
the cited code and ../CLAUDE.md yourself. Common false alarms: a serde rename already produces the right
name; errors are mapped centrally; the item belongs to a later MVP and isn't implemented; the test exists
under another name. For each finding answer: CONFIRMED or REJECTED, with one line of evidence.
Default to REJECTED when you cannot confirm it.
<findings>
```

## Report format
```
MVP <n> (desktop-agent): done | partial | blocked
Done:        <items>
Left:        <items + reason>
Checks:      fmt ✅ clippy ✅ test ✅ build ✅
Contract:    <confirmed>/<found> fixed · <rejected> false alarms · requests logged: <n>
Needs you:   <manual checks on the real laptop/phone>
Smoke test:  docs/smoke-tests/mvp-<n>.md
Next:        <command>
Commit:      feat(agent): MVP <n> - <summary>
```
