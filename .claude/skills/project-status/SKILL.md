---
name: project-status
description: Overview of am-remote from the root window. Shows roadmap progress of both projects, contract changes each project still has to apply, open contract requests and questions, spike decisions, conformance findings, and what to do next in which window. Use when the user asks where things stand or what to do next.
---

# /project-status

Read-only. Collect:
- `desktop-agent/CLAUDE.md` and `android-controller/CLAUDE.md`: the "Current status" checkboxes and the "Contract changes applied:" line
- `contract/CHANGELOG.md`: the newest `C-NNN` id, and what each entry applies to
- `contract/README.md`: "Open questions"
- `desktop-agent/docs/contract-requests.md` and `android-controller/docs/contract-requests.md`: unticked items
- `desktop-agent/docs/spikes/*.md`: the `**Decision:**` lines
- `contract/conformance-report.md`: date and open items per side
- Whether each project has code yet (`desktop-agent/Cargo.toml`, `android-controller/pubspec.yaml`)
- `git status --short` (if this is a git repo)

Answer briefly, in this shape:
1. **Roadmap**: table with MVP 0-3 as rows and desktop-agent / android-controller as columns (✅ / ⬜).
2. **Contract**: the newest id, then per project the applied id and what's pending.
3. **Open items**: open questions, project requests and conformance findings (counts plus one line each).
4. **Spikes**: link-playback and uia-queue, each with its decision or "not run".
5. **Next steps**: 1-4 items, each tagged with its window, e.g. `[root] /contract-change requests`, `[desktop-agent] /build-mvp 1`, `[android-controller] /sync-contract`.
