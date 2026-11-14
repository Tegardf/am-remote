# Smoke test — MVP 0: connect & pair (android-controller)

Needs a human, a real Android phone and the laptop running the desktop-agent's MVP 0.
Automated tests cover the logic with fakes; none of the steps below have been run on a device.

**Before you start**
- Laptop: agent running, Wi-Fi profile **Private**, firewall allows TCP 8765 and UDP 5353 (mDNS).
- Phone: same Wi-Fi (not a guest network), USB or wireless debugging on, `adb devices` lists it.
- From `android-controller/`: `flutter pub get`, `dart run build_runner build`, then `flutter run`.

| # | Do | Expect |
|---|---|---|
| 1 | Start the agent on the laptop, then `flutter run` on the phone. | The connect screen opens and shows a progress bar while searching. |
| 2 | Wait up to ~5 s. | The laptop's hostname appears in the list with `<ip>:8765`. |
| 3 | Tap it, type the 6-digit code from the agent's tray menu, tap **Pair**. | The Player screen opens. The app bar badge says **Connected** (green). The body names the laptop and whether Apple Music is running. |
| 4 | Force-stop the app and reopen it. | It goes straight to the Player screen (token kept) and shows **Connected**. |
| 5 | In the agent's tray, revoke this phone, then reopen the app (or wait ~10 s with it open). | The app goes back to the connect screen. Pairing again with a new code works. |
| 6 | On the pairing screen, enter a wrong 6-digit code. | "Wrong or expired code". The code is not echoed back in the message. |
| 7 | Enter fewer than 6 digits, or type letters. | Letters are ignored. Fewer than 6 digits shows "Enter all 6 digits" and sends nothing. |
| 8 | Go back, tap **Enter IP manually**, type the laptop's IP (optionally `:8765`), then pair. | Pairing works without discovery. An address like `http://x` is rejected. |
| 9 | Turn the phone's Wi-Fi off. | Within ~10 s the badge shows **Offline** (red) and the app stays on the Player screen. |
| 10 | Turn Wi-Fi back on. | Within ~10 s the badge returns to **Connected**. Tapping the badge re-checks immediately. |
| 11 | Stop the agent, then tap the badge. | **Offline**. Start the agent again and the badge returns to **Connected** without re-pairing. |
| 12 | With the app paired and open, make the laptop's IP change (reconnect its Wi-Fi so the router gives a new address, or move it to another network), and wait about 30–60 s. Needs the agent's C-004 (it re-publishes `ip`). | The badge goes **Offline**, then returns to **Connected** by itself: the app found the laptop at its new address and kept the pairing. It must not ask for a new code. |

**How the host is chosen (contract C-002):** Android's NSD often resolves the laptop only to an IPv6 link-local address (`fe80::…%wlan0`), while the agent listens on IPv4. So the agent's mDNS record carries a TXT field `ip=<ipv4>`, and the app uses that as the host. Without `ip` it takes an IPv4 address from the resolver, and otherwise skips the laptop. Needs the agent's C-002 (the `ip` field). **Not yet tried on a device.** If steps 1–2 still show nothing on your phone, use **Enter IP manually** (step 8) and note it as a failure of automatic discovery.

**Emulator:** the emulator's network is separate from your Wi-Fi, so discovery may list nothing there even with `ip`. Use **Enter IP manually** with `10.0.2.2`. Turning only Wi-Fi off doesn't test step 9 there, because the emulator falls back to mobile data and `10.0.2.2` stays reachable. Turn off both (`adb shell svc wifi disable` and `adb shell svc data disable`).

**Checked on the emulator (2026-11-14, against the real agent):** steps 3 (pair via `10.0.2.2`), 4, 9 and 10. Not checked: steps 1–2 (real-phone discovery), 5 (revoke in the tray), 6 (wrong code on a device), 8 (only the `10.0.2.2` form), 11.

**Also check by hand**
- `adb shell dumpsys package dev.amremote.am_remote | findstr allowBackup` shows backup off.
- The token appears nowhere in `adb logcat` while pairing and reconnecting.
- Android 7 (API 24) and a current Android version both start the app (minSdk 24).
