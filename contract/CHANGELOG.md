# Contract changelog

Only the **root window** writes this file, via `/contract-change`.
Each project applies new entries with `/sync-contract` in its own window, then records the newest id it applied in its own `CLAUDE.md` (the "Contract changes applied:" line).

Newest entry first. Entry format:

```
## C-NNN · YYYY-MM-DD · <title>
- **Applies to:** agent, app
- **Breaking:** no
- **Change:** <what changed in the contract section of CLAUDE.md>
- **Resolves:** <project requests / open question numbers, or "-">
- **Agent notes:** <hints for desktop-agent, or "-">
- **App notes:** <hints for android-controller, or "-">
```

---

## C-000 · 2026-11-14 · Contract v1 baseline
- **Applies to:** agent, app
- **Breaking:** no
- **Change:** The initial v1 contract as written in `CLAUDE.md`, with examples in `contract/samples.md`.
- **Resolves:** -
- **Agent notes:** Nothing to apply on its own. The contract is built phase by phase with `/build-mvp`.
- **App notes:** Same as the agent.
