# desktop-agent

Windows background app (Rust) that controls the Apple Music app and serves the am-remote API on your local network:
`http://<laptop-ip>:8765/api/v1`, discoverable over mDNS as `_amremote._tcp`. It runs in the tray and shows the pairing code there.

- Conventions, structure and implementation notes: [CLAUDE.md](CLAUDE.md)
- API contract (shared with the app): [../CLAUDE.md](../CLAUDE.md) and [../contract/](../contract/)

> **Status:** MVP 0, 1, 2 and 3 are done. MVP 4 (shuffle/repeat, queue, search through the app) is written and its checks are green; waiting for the manual smoke test in `docs/smoke-tests/mvp-4.md`.

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

## Install: start automatically at login
The agent runs as a normal background app in your own Windows session, started by Task Scheduler when you log in. It shows its tray icon and has no console window.

> **Why not a Windows service?** A service runs in "session 0", away from your desktop. From there it cannot see Apple Music's window (UI Automation), your media session (SMTC) or the tray. It would start but control nothing. A logon task gives the same "always running in the background" result and works.

Do these steps once in **PowerShell** (step 3 needs **Run as administrator**; open it from the Start menu with right-click → Run as administrator).

**1. Build the release `.exe`** (in this folder):
```powershell
cargo build --release
```

**2. Copy it to a fixed place**, so a later `cargo build` doesn't fight a running copy:
```powershell
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\am-remote" | Out-Null
Copy-Item .\target\release\am-remote-agent.exe "$env:LOCALAPPDATA\am-remote\am-remote-agent.exe" -Force
```

**3. Register the logon task** (PowerShell as administrator, logged in as the same Windows user that uses Apple Music):
```powershell
$exe       = "$env:LOCALAPPDATA\am-remote\am-remote-agent.exe"
$user      = "$env:USERDOMAIN\$env:USERNAME"
$action    = New-ScheduledTaskAction -Execute $exe
$trigger   = New-ScheduledTaskTrigger -AtLogOn -User $user
$trigger.Delay = 'PT30S'   # wait 30 s after login, so Wi-Fi and the taskbar are ready
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) `
               -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) `
               -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
               -MultipleInstances IgnoreNew -StartWhenAvailable
Register-ScheduledTask -TaskName "am-remote agent" -Action $action -Trigger $trigger `
  -Principal $principal -Settings $settings -Description "Apple Music remote agent (am-remote)"
```
What the settings do: `-ExecutionTimeLimit 0` stops Windows from killing it after the default 3 days; the restart settings retry up to 3 times if it fails; the battery settings keep it running on a laptop; `IgnoreNew` never starts a second copy.

**4. Firewall:** add the Private-network rule from "Run" above, if you haven't yet.

**5. Start it now** (no need to log out):
```powershell
Start-ScheduledTask -TaskName "am-remote agent"
```
Check: the am-remote icon is in the tray (maybe under the `^` overflow arrow), and `Get-ScheduledTaskInfo -TaskName "am-remote agent"` shows a recent `LastRunTime`. Then pair the phone with the code from the tray menu.

**6. Test a real login:** sign out and back in (or restart). About 30 s after login the tray icon appears and the phone reconnects by itself.

### Update to a new build
```powershell
cargo build --release
Stop-ScheduledTask -TaskName "am-remote agent"
Get-Process am-remote-agent -ErrorAction SilentlyContinue | Stop-Process
Copy-Item .\target\release\am-remote-agent.exe "$env:LOCALAPPDATA\am-remote\am-remote-agent.exe" -Force
Start-ScheduledTask -TaskName "am-remote agent"
```
Paired phones keep working: tokens live in `%APPDATA%\am-remote\config.json`, not next to the `.exe`.

### Stop, disable or remove
```powershell
Stop-ScheduledTask    -TaskName "am-remote agent"   # stop until next login (also quit it from the tray)
Disable-ScheduledTask -TaskName "am-remote agent"   # don't start at login any more
Enable-ScheduledTask  -TaskName "am-remote agent"
Unregister-ScheduledTask -TaskName "am-remote agent" -Confirm:$false   # remove completely (as administrator)
```
You can see and change the same task in the **Task Scheduler** app → Task Scheduler Library → `am-remote agent`.

### Notes
- Only one copy can run, because both would want port 8765. Quit the installed one (tray → Quit, or `Stop-ScheduledTask`) before `cargo run` during development.
- The release build has no console, so it shows no logs. To see logs, stop the task and run `cargo run` in a terminal.
- Windows sleep: the agent keeps running; the phone reconnects after the laptop wakes up.

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
| `/build-mvp [0-4]` | follows `.claude/tasks/mvp-<n>.md`: plan → build in small steps → cargo checks → contract review (reviewer + skeptic subagents) → tick status, log contract requests, write `docs/smoke-tests/mvp-<n>.md` |
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
| Installed agent doesn't start at login | Task Scheduler → `am-remote agent` → History / Last Run Result. Check the `.exe` path still exists, and that the task user is the one you log in with. Run `Start-ScheduledTask -TaskName "am-remote agent"` by hand |
| `cargo run` says the port is in use | The installed agent is running. Quit it from the tray or run `Stop-ScheduledTask -TaskName "am-remote agent"` |
| Phone can't find the laptop | Wi-Fi profile Private, firewall rule above, same network, no guest/AP isolation. Use manual IP in the app |
