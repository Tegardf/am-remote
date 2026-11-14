# desktop-agent

Windows background app (Rust) that controls the Apple Music app and serves the am-remote API on your local network:
`http://<laptop-ip>:8765/api/v1`, discoverable over mDNS as `_amremote._tcp`. It runs in the tray and shows the pairing code there.

- Conventions, structure and implementation notes: [CLAUDE.md](CLAUDE.md)
- API contract (shared with the app): [../CLAUDE.md](../CLAUDE.md) and [../contract/](../contract/)

> **Status:** MVP 0, 1, 2 and 3 are done. The roadmap is complete; MVP 3 passed its manual smoke test (`docs/smoke-tests/mvp-3.md`).

## What to install

| # | Tool | Why | How |
|---|---|---|---|
| 1 | **Windows 10/11 x64** | target platform | - |
| 2 | **Apple Music** (Microsoft Store app, not iTunes) | the app being controlled | Microsoft Store → "Apple Music" → install, sign in, play any song once |
| 3 | **Visual Studio 2022 Build Tools**, workload **"Desktop development with C++"** (MSVC v143 + Windows 11 SDK) | linker and Windows SDK used by Rust's MSVC toolchain and the `windows` crate | https://visualstudio.microsoft.com/visual-cpp-build-tools/ → install with that workload ticked. Install this **before** Rust |
| 4 | **Rust** via rustup, toolchain `stable-x86_64-pc-windows-msvc` | compiler, cargo, clippy, rustfmt | https://rustup.rs → run `rustup-init.exe` → default host `x86_64-pc-windows-msvc`. Then open a **new** terminal |
| 5 | VS Code extension **rust-analyzer** | editing, inline errors | VS Code Extensions → "rust-analyzer" |
| 6 | Node.js 18+ | Claude Code hook (rustfmt after each edit) | already installed |
| 7 | *Optional:* **Accessibility Insights for Windows** (or Inspect.exe from the Windows SDK) | look at Apple Music's UI Automation tree for `/spike uia-queue` | https://accessibilityinsights.io |

Check:
```powershell
rustup show active-toolchain   # stable-x86_64-pc-windows-msvc (default)
cargo --version
cargo clippy --version
rustfmt --version
```

## Run (after MVP 0)
```powershell
cargo run                     # debug build, logs in the console
cargo build --release         # target\release\am-remote-agent.exe, runs without a console window
```

**Firewall:** on first run Windows asks whether to allow network access. Allow it on **Private** networks only. To add the rule by hand (PowerShell as Administrator):
```powershell
New-NetFirewallRule -DisplayName "am-remote agent" -Direction Inbound -Protocol TCP -LocalPort 8765 -Profile Private -Action Allow
```
Also set your Wi-Fi to Private: Settings → Network & internet → Wi-Fi → (your network) → Network profile type → **Private**.

## Try the API without the phone
```powershell
$base = "http://localhost:8765/api/v1"
$r = Invoke-RestMethod -Method Post "$base/pair" -ContentType 'application/json' -Body '{"code":"<code from tray>","device_name":"pc-test"}'
$h = @{ Authorization = "Bearer $($r.token)" }
Invoke-RestMethod "$base/status" -Headers $h
Invoke-RestMethod -Method Post "$base/player/toggle" -Headers $h      # MVP 1
Invoke-RestMethod "$base/volume" -Headers $h                          # MVP 1
```
(In PowerShell, `curl` is an alias for `Invoke-WebRequest`. Use `curl.exe` if you want real curl.)

## Configuration
`%APPDATA%\am-remote\config.json` holds the port, paired-device tokens, and autostart. **It contains tokens**, so don't share or commit it. Revoke a phone from the tray menu.

## Checks
```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```
Or run `/verify` in Claude Code.

## Claude Code in this window
Open **this folder** (`desktop-agent/`) in its own VS Code window.

| Command | What it does |
|---|---|
| `/build-mvp [0-3]` | follows `.claude/tasks/mvp-<n>.md`: plan → build in small steps → cargo checks → contract review (reviewer + skeptic subagents) → tick status, log contract requests, write `docs/smoke-tests/mvp-<n>.md` |
| `/verify` | fmt check, clippy, test, build, and a report |
| `/sync-contract` | apply new `../contract/CHANGELOG.md` entries and `../contract/conformance-report.md` items |
| `/spike link-playback` | follows `.claude/tasks/spike-link-playback.md`: can a `music.apple.com` link start playback? Decides the MVP 3 approach. Run before MVP 3; it will play music |
| `/spike uia-queue` | follows `.claude/tasks/spike-uia-queue.md`: can UI Automation read the playlist and queue? Optional, fills `playlist_name` |

Also in `.claude/`:
- **Tasks:** `tasks/*.md`, step-by-step instructions for each phase and spike, opened only when a command runs. `tasks/_workflow-rules.md` holds the rules every task follows.
- **Rules:** `scope.md` (edit only this folder, contract requests, real-machine facts), `api-contract.md` (wire format), `security.md` (tokens, pairing, logging)
- **Hook:** rustfmt after each edit
- **Settings:** cargo commands pre-allowed; edits to `../CLAUDE.md`, `../contract/` and the app are denied

This window never changes the contract. Gaps go to `docs/contract-requests.md`, and the root window settles them.

## Troubleshooting
| Problem | Fix |
|---|---|
| `link.exe not found` / `LNK1181` | Build Tools "Desktop development with C++" workload missing, or the GNU toolchain is active: `rustup default stable-x86_64-pc-windows-msvc` |
| `cargo` not recognised after installing | Open a new terminal or VS Code window (PATH is read at start) |
| Agent says Apple Music is not running | It must be the Microsoft Store **Apple Music** app (not iTunes), and it must be running |
| Volume endpoints fail before anything has played | Windows only creates Apple Music's audio session after its first playback. They answer `409 NO_AUDIO_SESSION` (playback buttons still work). Play something once |
| Phone can't find the laptop | Wi-Fi profile Private, firewall rule above, same network, no guest/AP isolation. Use manual IP in the app |
