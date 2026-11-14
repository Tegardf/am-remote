# Smoke test — MVP 2: now playing and live updates (android-controller)

Needs a human, the phone paired with the laptop, and the desktop-agent's **MVP 2** (`/player/state`, `/player/artwork`, `/player/seek`, `/ws`).
The automated tests cover the logic with fakes, including a fake WebSocket and fake time. The steps marked ✅ were also run on the emulator against the real agent (2026-11-14); the others have not been run on a device.

**Before you start**
- Agent running with its MVP 2 endpoints, and Apple Music playing a track that has artwork.
- App paired. From `android-controller/`: `flutter run`.
- **Android 13+:** allow notifications for AM Remote when asked (Settings → Apps → AM Remote → Notifications otherwise). Without it the media notification is not shown.

| # | Do | Expect |
|---|---|---|
| 1 ✅ | Open the app. | The current song appears with its artwork, title, "artist · album", a progress bar with times, and the play/pause button shows **pause** while playing. The bar moves on its own. |
| 2 | Skip a track **on the laptop**. | Within ~1 s the phone shows the new title and new artwork. |
| 3 ✅ | Tap play/pause on the phone. | The laptop pauses and, after the push, the icon turns to **play** and the bar stops. The icon does not change before the push arrives. |
| 4 | Drag the progress bar and let go. | The laptop jumps to that position. The bar stays on the target and then follows the laptop's pushed position, with no jump back. |
| 5 | Lock the phone. | The lock screen shows the song with working previous, play/pause and next. (✅ in the notification shade: title, artist, progress, three buttons; pressing **pause** there paused the laptop and the app followed.) |
| 6 | Change the volume on the laptop (mixer). | The phone's volume slider follows within ~1 s, unless you are dragging it. |
| 7 | Close Apple Music on the laptop. | "Open Apple Music on your laptop" appears, and the media notification goes away. Reopen it: the controls and notification come back without restarting the app. |
| 8 | Turn the phone's Wi-Fi off for 30 s, then on. | A red "No connection to your laptop" banner and the **Offline** badge appear and the media notification goes away. After reconnecting, they go away and the notification returns. No re-pairing. |
| 9 | Restart the agent. | The app reconnects without being re-paired. |
| 10 | Swipe the app away from recent apps. | The media notification disappears (its buttons could not work any more). |
| 11 | Revoke this phone in the agent's tray. | Within a few seconds the app is back on the connect screen. |
| 12 | A track with no artwork. | A grey placeholder with a note icon, no error. |

**Not built:** the notification shows no artwork picture (title, artist, album and progress only). The hardware volume keys are not wired (needs native code that can't be tested without a device).

**If the notification never shows:** it needs the `WAKE_LOCK`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK` and `POST_NOTIFICATIONS` permissions, all in the manifest. On the emulator it failed silently until `WAKE_LOCK` was added.

**Re-discovery:** after three failed health checks in a row (about 30 s) the app looks for the laptop again by its name and follows it to a new address. It does nothing for an address typed by hand, because that has no name to match.
