# am-remote

Control the **Apple Music app on a Windows laptop** from an **Android phone** over local Wi-Fi.
Music always plays on the laptop; the phone is only a remote.

> Personal project, not affiliated with Apple. It uses only public Windows APIs (media session, per-app audio volume, UI Automation). It doesn't hook into Apple Music, touch DRM, or call Apple's private APIs.

```
  Android phone                                   Windows laptop
 ┌──────────────────────┐   mDNS discovery    ┌───────────────────────────┐
 │  android-controller  │ ──────────────────► │  desktop-agent (.exe)     │
 │  (Flutter)           │  HTTP :8765 + WS    │  tray icon, pairing code  │
 │                      │ ◄─────────────────► │  SMTC · Core Audio · UIA  │
 └──────────────────────┘                     └─────────────┬─────────────┘
                                                            ▼
                                                     Apple Music app
```

## Status

| Phase | What | desktop-agent | android-controller |
|---|---|---|---|
| MVP 0 | Foundation: server, tray, pairing, mDNS · connect screen | ⬜ | ⬜ |
| MVP 1 | Play, pause, next, previous, volume, mute | ⬜ | ⬜ |
| MVP 2 | Now playing, artwork, seek, live updates, media notification | ⬜ | ⬜ |
| MVP 3 | Playlists and search (after the link-playback spike) | ⬜ | ⬜ |

The source of truth is the "Current status" list in each project's `CLAUDE.md` (`/project-status` in the root window summarises it).

## Repository layout

```
am-remote/
├─ CLAUDE.md              roadmap + API contract (source of truth); loaded in every window, so kept short
├─ contract/              samples.md (example JSON per shape), CHANGELOG.md, open questions
├─ .claude/               root maintenance: skills, tasks/contract-check.md, rules, check-setup script
├─ desktop-agent/         Rust Windows agent   (own CLAUDE.md, README, .claude/)
│  └─ .claude/tasks/      _workflow-rules.md, mvp-0..3.md, spike-*.md
└─ android-controller/    Flutter Android app  (own CLAUDE.md, README, .claude/)
   └─ .claude/tasks/      _workflow-rules.md, mvp-0..3.md
```

Everything Claude follows is plain Markdown. The only exception is `.claude/settings.json` (permissions and hooks), which Claude Code requires in JSON.

### What loads at session start (kept small) vs. on demand
| Loaded at every session start | Loaded only when needed |
|---|---|
| `CLAUDE.md` files (root + the project's) | `.claude/tasks/*.md`: opened by `/build-mvp`, `/spike`, `/contract-check` |
| the short `scope.md` rule (project windows) | path-scoped rules: `api-contract.md`, `security.md`, `claude-setup.md`, when matching files are edited |
| one-line skill descriptions | `contract/samples.md`, `contract/CHANGELOG.md`: read during a task |

## How to work on it: three VS Code windows

Open each folder in **its own VS Code window** (File → Open Folder). Claude Code in each window uses that folder's `.claude/` setup, plus the root `CLAUDE.md`, which loads automatically everywhere.

| Window | Purpose | Commands |
|---|---|---|
| `am-remote/` (this folder) | **Maintenance only.** Roadmap, API contract, the Claude setup of both projects, cross-project checks. No project code is written here (the root settings deny edits to `src/`, `lib/`, `test/`). | `/project-status` · `/contract-change` · `/contract-check` · `/new-task` |
| `desktop-agent/` | Develop the Rust agent | `/build-mvp` · `/verify` · `/sync-contract` · `/spike` |
| `android-controller/` | Develop the Flutter app | `/build-mvp` · `/verify` · `/sync-contract` |

### How the windows hand off work
Each window writes only the files it owns:
- **Contract change:** root `/contract-change` updates `CLAUDE.md`, `contract/samples.md` and `contract/CHANGELOG.md`. Each project then runs `/sync-contract`.
- **Contract problem found while coding:** the project logs it in its own `docs/contract-requests.md`. Root `/contract-change requests` settles it.
- **Conformance:** root `/contract-check` writes `contract/conformance-report.md`, and projects fix their items with `/sync-contract`.
- **Spikes:** desktop-agent `/spike` writes `desktop-agent/docs/spikes/<name>.md`. The link-playback decision gates MVP 3.

### Commands in detail
| Command | Window | Follows | What it does |
|---|---|---|---|
| `/build-mvp [0-3]` | project | `tasks/_workflow-rules.md` + `tasks/mvp-<n>.md` | plan → build in small steps → checks → contract review (reviewer + skeptic subagents) → tick status, log contract requests, write `docs/smoke-tests/mvp-<n>.md` |
| `/verify` | project | the skill | format, lint, test, build, and a report |
| `/sync-contract` | project | the skill | apply new `contract/CHANGELOG.md` entries and conformance findings |
| `/spike link-playback\|uia-queue` | desktop-agent | `tasks/spike-<name>.md` | inspect the app + research → probe on this machine → decision report |
| `/project-status` | root | the skill | progress, pending contract changes, open requests, next step per window |
| `/contract-change <what>\|requests` | root | the skill | propose → confirm → update contract, samples, changelog |
| `/contract-check [mvp]` | root | `tasks/contract-check.md` | two read-only finder subagents → skeptic per side → `contract/conformance-report.md` |
| `/new-task <project> <what>` | root | the skill | create or update a task, rule, skill or hook for a project, and validate it |

Hooks format `.rs`/`.dart` files after each edit in the project windows. In the root window, a hook validates `contract/samples.md`, task files and settings on each edit (`node .claude/scripts/check-setup.mjs`).

## What to install

Nothing here is installed automatically. Install these yourself; each project README has the details.

**Everywhere**
| Tool | Needed for | Status on this laptop (2026-11-14) |
|---|---|---|
| Git | version control | ✅ installed (2.55) |
| VS Code + Claude Code extension | the three windows and their commands | ✅ |
| Node.js 18+ | Claude Code hooks and `.claude/scripts/check-setup.mjs` | ✅ installed (v24) |

**desktop-agent** (see [desktop-agent/README.md](desktop-agent/README.md#what-to-install))
| Tool | Status |
|---|---|
| Apple Music app (Microsoft Store), signed in | check |
| Visual Studio 2022 Build Tools, workload "Desktop development with C++" | ❌ install |
| Rust via rustup, toolchain `stable-x86_64-pc-windows-msvc` | ❌ install |
| VS Code extension: rust-analyzer | install |
| Optional: Accessibility Insights for Windows (for the UIA spike) | optional |

**android-controller** (see [android-controller/README.md](android-controller/README.md#what-to-install))
| Tool | Status |
|---|---|
| Flutter SDK (stable; includes Dart) on `PATH` | ❌ install |
| Android Studio (Android SDK, platform-tools, command-line tools, bundled JDK) + `flutter doctor --android-licenses` | ❌ install |
| Android phone, Android 7.0+ (API 24), USB or wireless debugging on | check |
| VS Code extension: Flutter (installs Dart) | install |

**Check everything:**
```powershell
git --version; node --version
rustup show active-toolchain   # ...stable-x86_64-pc-windows-msvc (default)
cargo --version
flutter doctor                 # Flutter ✓, Android toolchain ✓ (licenses accepted)
adb devices                    # your phone listed as "device"
```

**Network:** the laptop's Wi-Fi profile must be **Private**, Windows Firewall must allow the agent (TCP 8765; mDNS uses UDP 5353), and phone and laptop must be on the same Wi-Fi. Guest networks often block device-to-device traffic.

## Getting started
1. Install the tools above. Then optionally `git init` in this folder and make a first commit, so every task run can be reviewed as a diff.
2. **Root window:** `/project-status`, then `/contract-change requests` to settle the open questions in `contract/README.md`. At least #1 (no error code for validation) matters for MVP 0.
3. **desktop-agent window:** `/build-mvp 0`. **android-controller window:** `/build-mvp 0`. The two can run at the same time.
4. Run the agent, allow it through the firewall, run the app on the phone, pair with the code from the tray. Follow `docs/smoke-tests/mvp-0.md` in each project.
5. Continue with `/build-mvp 1`, then `/build-mvp 2`. Next, `/spike link-playback` in desktop-agent, then `/build-mvp 3` in both projects. Run `/contract-check` in the root window after each phase.

## Known limits
- Windows' media session API (SMTC) exposes only the current track: no queue, no playlists, no volume. Volume comes from per-app Core Audio.
- Playlists and the queue need UI Automation of the Apple Music window, which can break when Apple updates the app. It is isolated behind a cargo feature and answers `501 NOT_SUPPORTED` when it fails.
- Out of scope: hooking or injecting into Apple Music, DRM, Apple's private server API.
