# Task — cross-project contract check (root window)

**Read-only for project code.** The only file this task writes is `contract/conformance-report.md`.

## Inputs
- **MVP `<n>`:** from the arguments. Otherwise the highest MVP ticked `[x]` in **both** `desktop-agent/CLAUDE.md` and `android-controller/CLAUDE.md` (0 if none is). Items of later phases are checked only where they're already implemented.
- **Present sides:** `desktop-agent/Cargo.toml` → agent present; `android-controller/pubspec.yaml` → app present. If neither exists, say there's no code yet and stop.

## Steps
1. **Find:** spawn two **read-only** `general-purpose` subagents in parallel, one per lens. Use the *Finder prompt* below, with the lens text filled in.
   - **Lens A, HTTP:** every endpoint in the contract table: method, path, port, 204 vs 200, request and response bodies, JSON names after serde / json_serializable renames, int ms, volume 0-100, nullable `playlist_name`, status enum; Bearer auth except `/pair`; the error body and codes (on the agent including framework rejections; on the app 204 not decoded and errors mapped by code).
   - **Lens B, realtime and pairing:** WebSocket `/ws?token=`, the `{type,data}` envelope, `state`/`volume`/`apple_music`, state on connect, ping every 15 s / pong, unknown types ignored; mDNS service type, port and TXT keys on both sides; the pairing flow, token format and persistence; 401 handling in the app.
2. **Dedupe** the findings by side + file + contract item.
3. **Verify:** spawn one **read-only** skeptic subagent per side that has findings (`agent`, `app`, `both`), using the *Skeptic prompt*. Keep only the CONFIRMED ones. Findings of kind `contract-gap` skip verification and go to "Contract gaps".
4. **Write `contract/conformance-report.md`** (overwrite):
   ```
   # Contract conformance report
   Date: YYYY-MM-DD · Checked up to MVP <n> · Agent code: yes/no · App code: yes/no

   ## desktop-agent
   - [ ] #<id> <file>:<line> - <contract item> - expected <…>; actual <…>
   ## android-controller
   ## both (the projects disagree with each other)
   ## Contract gaps (settle with /contract-change)
   ## Unverified (the skeptic could not decide; check by hand)
   ```
   Leave out empty sections.
5. **Report** the counts per side and the false alarms filtered out. Then give the next step per window, e.g. `[desktop-agent] /sync-contract`, or `[root] /contract-change requests` for gaps.

## Finder prompt
```
Read-only: do not edit any file. You are in the am-remote root.
Contract: CLAUDE.md "API contract (v1)" (source of truth); examples: contract/samples.md; open questions:
contract/README.md. Agent: desktop-agent/ (Rust). App: android-controller/ (Flutter).
Lens: <lens text>. Contract items with MVP <= <n> must exist; later ones only if implemented.
Also compare the two projects with each other: report where the app sends/expects something the agent
does not produce (side "both"). Report kind "missing-test" if a present project has no test that
round-trips contract/samples.md for the shapes it implements; kind "contract-gap" (side "contract") if
the contract is silent/contradictory on something the code had to decide.
Each finding: side (agent|app|both|contract), kind (mismatch|missing-test|contract-gap), file:line,
contract item, expected, actual (quote the code). Matching behaviour and style are not findings.
```

## Skeptic prompt
```
Read-only: do not edit any file. You are in the am-remote root. Try to refute each finding below by opening
the cited code and CLAUDE.md yourself. Common false alarms: a rename attribute or build.yaml option already
produces the right name; errors are mapped centrally; the item is from a later MVP and not implemented; the
test exists under another name. For each: CONFIRMED or REJECTED with one line of evidence. Default to
REJECTED when you cannot confirm it.
<findings>
```
