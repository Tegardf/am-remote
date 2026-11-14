# Smoke test — MVP 1: player controls (android-controller)

Needs a human, the phone (or emulator) paired with the laptop, and the desktop-agent's **MVP 1** (transport and volume endpoints).
The automated tests cover the logic with fakes (including the slider throttling in fake time). None of the steps below have been run on a device.

**Before you start**
- Agent running with its MVP 1 endpoints, and Apple Music open and playing on the laptop.
- App paired (MVP 0 smoke test). From `android-controller/`: `flutter run`.
- Open the Windows volume mixer on the laptop so you can watch Apple Music's own level.

| # | Do | Expect |
|---|---|---|
| 1 | Open the app. | Player screen: previous, a big play/pause, next, then a volume row (mute button, slider, number). The slider shows the laptop's current Apple Music level, not 0. |
| 2 | Tap play/pause, next, previous. | Each acts on Apple Music within ~1 s. The buttons are disabled for the short moment a request is running. The icon doesn't change (the now-playing state arrives in MVP 2). |
| 3 | Drag the slider slowly, then quickly. | The laptop volume follows without lag or a flood of delayed jumps. Release at 30: the mixer shows 30 and the number reads 30. |
| 4 | Tap mute. | Only Apple Music is muted (check the mixer). The icon changes to "volume off". Tap again: unmuted, at the previous level. |
| 5 | Close Apple Music on the laptop. | Within ~10 s the screen shows "Open Apple Music on your laptop" with a **Retry** button. |
| 6 | Reopen Apple Music, tap **Retry**. | The controls come back, and the slider shows the current level. |
| 7 | Tap play/pause while Apple Music is closed (before the 10 s refresh). | The same empty state appears at once, and the connection badge stays "Connected". |
| 8 | Stop the agent, tap play/pause. | A short snackbar "Can't reach the laptop", and the badge turns **Offline**. Start the agent: the badge returns by itself. |
| 9 | Revoke the phone in the agent's tray, then tap play/pause. | The app goes back to the connect screen (401 clears the token). |

**Checked on the emulator against the real agent (2026-11-14):** steps 1 (the slider loads the laptop's level), 2 (previous, play/pause and next work, confirmed by the user), 3 (the slider sets the level, which the agent then reports back after an app restart) and 4 (mute and unmute, also persisted across a restart). Not checked on a device: steps 5–9 (the empty state, Retry, offline snackbar, revoke). Tests cover them with fakes.

**Not covered here:** hardware volume keys controlling the laptop. That needs native Android code that can't be tested without a device, so it isn't built in this phase.

**No audio session yet (contract C-003):** Apple Music is open, but nothing has played since it started, so Windows has no audio session for it and the agent answers `409 NO_AUDIO_SESSION` to the volume calls. Needs the agent's C-003. **Not yet tried on a device.**

| # | Do | Expect |
|---|---|---|
| 10 | Quit Apple Music, start it again, and open the app **without playing anything**. | The Player screen stays: play/pause, next and previous are usable. The volume slider and mute button are greyed out, with the note "Volume works once Apple Music has played something". It is **not** the "Open Apple Music on your laptop" screen. |
| 11 | Press play on the phone. | Within a second or two the slider and mute button come back and show the current level. |
| 12 | Quit Apple Music while the app is open. | The "Open Apple Music" screen. Start it again and play something: the controls come back with the current level, not the one from before. |
| 13 | Turn the phone's Wi-Fi off and on again (or restart the agent) while the app is open. | After it reconnects, the slider shows the laptop's current level, even if you changed it on the laptop meanwhile. |
