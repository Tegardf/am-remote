# desktop-agent tasks

Plain-Markdown task files. Each one says what to build, the gate, the tests, when it's done, and the manual smoke test.
None of them load at session start: the commands open them on demand.

| Order | Task file | Run with | Notes |
|---|---|---|---|
| 1 | [mvp-0.md](mvp-0.md) | `/build-mvp 0` | Scaffold, server, auth, pairing, mDNS, tray |
| 2 | [mvp-1.md](mvp-1.md) | `/build-mvp 1` | SMTC controls, per-app volume |
| 3 | [mvp-2.md](mvp-2.md) | `/build-mvp 2` | Player state, artwork, seek, WebSocket push |
| 3b | [spike-uia-queue.md](spike-uia-queue.md) | `/spike uia-queue` | Optional; fills `playlist_name` |
| 4 | [spike-link-playback.md](spike-link-playback.md) | `/spike link-playback` | **Required before MVP 3** |
| 5 | [mvp-3.md](mvp-3.md) | `/build-mvp 3` | Playlists, search, play track |
| 6 | [spike-mvp4.md](spike-mvp4.md) | `/spike mvp4` | **Required before MVP 4**: shuffle/repeat, queue, app search (done) |
| 7 | [mvp-4.md](mvp-4.md) | `/build-mvp 4` | Modes, queue, app search (contract C-006) |

Every task follows [_workflow-rules.md](_workflow-rules.md): plan → build → check → contract review → wrap-up.
To add or change a task, use `/new-task` in the **root** window.
