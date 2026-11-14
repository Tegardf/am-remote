# am-remote

> **Disclaimer:** this is a fully vibe-coded project, made just for fun and to solve my own problem. It isn't polished, supported or reviewed for anyone else's use, and it comes with no guarantees. It isn't affiliated with Apple. Use it, fork it or learn from it at your own risk.

Control the **Apple Music app on a Windows laptop** from an **Android phone** over your local Wi-Fi.
The music always plays on the laptop. The phone is only a remote.

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

## Why
I wanted to skip, pause and change the volume of Apple Music on my laptop from the couch, without a streaming-box or casting setup. The Windows app has no remote, so I built one.

## What it does
- Play, pause, next, previous, volume and mute.
- Now playing: title, artist, album, artwork, progress and seek, updated live.
- A media notification and lock-screen controls on the phone.
- Browse your playlists and play one; search the catalog and play a song.
- The phone finds the laptop by itself (mDNS) and pairs with a 6-digit code shown in the laptop's tray menu.

## Parts

| Folder | What | Stack | Docs |
|---|---|---|---|
| [`desktop-agent/`](desktop-agent/) | Background `.exe` on the laptop that talks to Apple Music and serves the API on the LAN | Rust (tokio, axum, windows-rs) | [README](desktop-agent/README.md) |
| [`android-controller/`](android-controller/) | The Android remote app | Flutter | [README](android-controller/README.md) |
| [`contract/`](contract/) | Example JSON for every API message, a changelog, and open questions | Markdown | [README](contract/README.md) |

The API both sides follow is written in [`CLAUDE.md`](CLAUDE.md) ("API contract (v1)"). That section is the source of truth: HTTP and WebSocket on port 8765, pairing token, error format.

## Status

| Phase | What | Built |
|---|---|---|
| MVP 0 | Foundation: server, tray, pairing, mDNS · connect screen | ✅ |
| MVP 1 | Play, pause, next, previous, volume, mute | ✅ |
| MVP 2 | Now playing, artwork, seek, live updates, media notification | ✅ |
| MVP 3 | Playlists and search | ✅ |

"Built" means the code exists and its checks passed. How well it works on a real laptop and phone is covered by the manual smoke-test checklists in each project's `docs/smoke-tests/`.

## What to install
Nothing is installed for you. Each project README has the full steps.

| Tool | For |
|---|---|
| **Windows 10/11** and the **Apple Music** app from the Microsoft Store (signed in) | the laptop |
| **Visual Studio 2022+ Build Tools** with "Desktop development with C++", then **Rust** via rustup (`stable-x86_64-pc-windows-msvc`) | building `desktop-agent` |
| **Flutter SDK** (stable) and **Android Studio** (Android SDK, command-line tools, licenses accepted) | building `android-controller` |
| An **Android phone**, Android 7.0+ (API 24), on the same Wi-Fi | running the app |
| Git, Node.js 18+ | repository tooling |

Quick check:
```powershell
cargo --version
flutter doctor        # Flutter and Android toolchain should be ✓
```

**Network:** set the laptop's Wi-Fi profile to **Private**, allow the agent through Windows Firewall (TCP 8765; mDNS uses UDP 5353), and keep phone and laptop on the same Wi-Fi. Guest networks often block device-to-device traffic.

## Try it
1. Start the agent on the laptop (see [desktop-agent/README.md](desktop-agent/README.md)). A tray icon appears with a 6-digit pairing code.
2. Run the app on the phone (see [android-controller/README.md](android-controller/README.md)). The laptop shows up in the list, or enter its IP by hand.
3. Type the pairing code. From now on the phone is paired until you revoke it from the tray menu.
4. Open Apple Music on the laptop, play something, and control it from the phone.

## Known limits
- Windows' media session API (SMTC) exposes only the current track: no queue, no playlists, no volume. Volume comes from per-app Core Audio instead.
- Playlists and playing a song need UI Automation of the Apple Music window, which is fragile and can break when Apple updates the app. When it fails, the agent answers `501 NOT_SUPPORTED` instead of crashing.
- Playing a song or playlist visibly changes Apple Music's window, because it's done through the app's own UI.
- It's meant for a trusted home network only: traffic is plain HTTP, and the pairing token is the only protection.
- Out of scope: hooking or injecting into Apple Music, touching DRM, or imitating Apple's private server API.
