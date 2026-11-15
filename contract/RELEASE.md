# Release plan: v1 freeze (am-remote)

## Now: v0.2.0 (MVP 4) — written 2026-11-15
v0.1.0 was committed (`111bc09`, not yet tagged). MVP 4 (C-006) is built in both projects, the last `/contract-check` found no breaking mismatch, and **C-007 is the last contract change for v0.2.0: the contract is frozen again** (same freeze rule as section 3: later findings go to the backlog, not the contract).

| # | Window | What |
|---|---|---|
| 1 | desktop-agent | `/sync-contract` (C-007: timeout → `500`, conformance #1), then `/verify` in both modes (with and without `uia`) |
| 2 | android-controller | `/sync-contract` (marker C-007, conformance #2: emoji minimum), tick MVP 4 in `./CLAUDE.md` once its smoke test passed |
| 3 | android-controller | new launcher icon: "follow `.claude/tasks/app-icon.md`" |
| 4 | both | versions: `desktop-agent/Cargo.toml` → `version = "0.2.0"`; `android-controller/pubspec.yaml` → `version: 0.2.0+2` (the `+2` build number must be higher than the current `+1`) |
| 5 | both | `/verify`, then `cargo build --release` and `flutter build apk --release` |
| 6 | laptop + phone | `docs/smoke-tests/mvp-4.md` in both projects, plus a quick MVP 0–3 pass |
| 7 | laptop | install the agent to start at login: `desktop-agent/README.md` → "Install: start automatically at login" |
| 8 | root | commit, then tag: `git tag v0.1.0 111bc09` (optional, marks the old one) and `git tag v0.2.0` on the release commit |

Known limits shipped in v0.2.0 (in the contract, not bugs): the queue shows only about the next 20 songs; tapping a queue row skips through the songs before it; app-search results have no artwork; catalog playlists, artists and other kinds from app search answer "can't play yet"; queue and search use visibly move the Apple Music window.

---

The sections below are the v0.1.0 plan, kept for reference.

Written 2026-11-14 after the last `/contract-check` (contract at C-004, both projects applied it).
Goal: stop the "fix, then the contract changes again" loop and define the last change before a release version.

## 1. Why it looped
Each review compared the code with the contract in more detail and found one more unwritten decision. Every one of them became a new contract change after the code was already done. From here on the order is reversed: **the contract is decided first, once (C-005 below), and it is written to match the code that already exists.** C-005 needs **no code change in either project.**

## 2. The last contract change: C-005 (documentation only) — APPLIED 2026-11-14
Apply with `/contract-change` (tell it: "apply C-005 from contract/RELEASE.md"). Additive, stays in v1. Adds to the "Input rules" section of `CLAUDE.md`:

1. **Order of checks, play-track exception.** "`POST /play/track` is the second exception to the order of checks: it first looks the song up in the catalog (needed for the title), so `404` (unknown song) and `500` (laptop offline) can come before `409`. `POST /play/playlist` follows the normal order."
2. **`501` before `409`.** "`501 NOT_SUPPORTED` (UI Automation unavailable) is answered before `409`."
3. **Failed media backend.** Replace "media calls answer `500 INTERNAL`" with: "If the agent's own media backend failed to start, `apple_music_running` is `false` and media calls fail. Which of `500`, `409` or `501` an endpoint answers is not specified; a client treats any non-2xx as failure."
4. **Discovery fallback scope.** "The resolver-IPv4 fallback applies to a saved (paired) session. When pairing from the discovery list the client may try only the `ip` host; the user can retry or enter the address by hand."

Changelog entry **C-005**: *Applies to:* none (documentation only). *Resolves:* all four contract gaps in the last conformance report. Agent and app only record the marker C-005 with `/sync-contract`.

## 3. Freeze rule (this is what ends the loop)
- After C-005 the **v1 contract is frozen** until the release is tagged. Do **not** run `/contract-check` again until the release gate below is green; run it **once** after that, as the final check.
- A finding from a later review goes into **section 5 (backlog)**. It becomes a contract change only if it is a user-visible bug or a breaking difference between the two projects.
- A contract change after the release is a new version (`v1.1`), not a loop.

## 4. Release gate (everything here must be true before tagging)

### Solved (verified in code by the contract checks)
- All 16 routes, 9 error codes, input rules, order of checks, pairing rules, the WebSocket connect order, `ip` TXT with refresh, `NO_AUDIO_SESSION`, volume staleness, seek with unknown duration, the 45 s timeout, HTTPS-only thumbnails. All 28 samples round-trip in both projects.

### Missing before release (blockers)
| # | Where | What | How |
|---|---|---|---|
| 1 | both | `/verify` green on the final code | run `/verify` in each window (cargo fmt, clippy, test, build; flutter format, analyze, test) |
| 2 | desktop-agent | release build runs | `cargo build --release`; start the `.exe`, tray icon shows, no console window |
| 3 | android-controller | release APK installs | `flutter build apk --release`, install on the phone |
| 4 | both | smoke tests on the **real laptop and phone**, MVP 0–3 | the checklists in each project's `docs/smoke-tests/` |
| 5 | desktop-agent | **seek** actually moves Apple Music | Apple Music reports `IsPlaybackPositionEnabled = false`, so the call is accepted but the audio may not jump. If it does not, say so in the README ("seek not supported") and return `501 NOT_SUPPORTED` |
| 6 | desktop-agent | playlist ids survive an Apple Music restart | unverified; restart Apple Music, list playlists, play one by an old id |
| 7 | both | first-time setup works from the README | fresh Wi-Fi, firewall on Private, pair, control |
| 8 | root | version numbers | agent `Cargo.toml` and app `pubspec.yaml` both at the release version (see below) |

Blockers 1–4 are the real gate. 5 and 6 are the two known unknowns on the real machine.

### Version
Suggest **v0.1.0** for both (first working version, personal project). Tag the repo `v0.1.0` after the gate. Put the number in `desktop-agent/Cargo.toml` (`agent_version` reports it) and `android-controller/pubspec.yaml`. If you prefer `v1.0.0`, the contract is already named v1, so it is only a label.

## 5. Backlog (optional polish, not blockers; do after the release or when bored)
**desktop-agent**
- Tests: failed-backend behaviour, no-body `400` on volume/seek/play-track, `q=a+b`, `{"muted":null}`.
- Smoke-test line for the `ip` re-announce (`docs/smoke-tests/mvp-0.md`).
- Make `/play/track` check the window before the catalog lookup (would remove exception 1 of C-005, but only worth it if offline behaviour matters to you).

**android-controller**
- Test: load the volume first, then answer `APPLE_MUSIC_NOT_RUNNING`, and assert the level is stale (the current test passes vacuously).
- Guard: a test that fails when a new key is added to `samples.md`.
- Try all discovered hosts when pairing from the discovery list (would remove C-005 item 4).

## 6. Order of work from here
1. Root: `/contract-change` → apply C-005. Commit: `docs(contract): C-005 final v1 clarifications`.
2. Both project windows: `/sync-contract` (records C-005 only), then `/verify`. Commit each project.
3. Release gate blockers 1–8, with the real laptop and phone.
4. Root: one last `/contract-check`; file any finding in section 5.
5. Tag the release.
