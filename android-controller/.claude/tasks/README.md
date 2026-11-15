# android-controller tasks

Plain-Markdown task files. Each one says what to build, the gate, the tests, when it's done, and the manual smoke test.
None of them load at session start: the commands open them on demand.

| Order | Task file | Run with | Notes |
|---|---|---|---|
| 1 | [mvp-0.md](mvp-0.md) | `/build-mvp 0` | Scaffold, connect, discovery, pairing |
| 2 | [mvp-1.md](mvp-1.md) | `/build-mvp 1` | Player controls, volume, mute |
| 3 | [mvp-2.md](mvp-2.md) | `/build-mvp 2` | WebSocket, now playing, seek, media notification |
| 4 | [mvp-3.md](mvp-3.md) | `/build-mvp 3` | Bottom nav, playlists, search |
| 5 | [mvp-4.md](mvp-4.md) | `/build-mvp 4` | Shuffle/repeat, Up Next list, app search (contract C-006) |
| – | [app-icon.md](app-icon.md) | ask: "follow `.claude/tasks/app-icon.md`" | New launcher icon from `music-controller-icon.svg` (before the v0.2.0 release) |

Every task follows [_workflow-rules.md](_workflow-rules.md): plan → build → check → contract review → wrap-up.
To add or change a task, use `/new-task` in the **root** window.
