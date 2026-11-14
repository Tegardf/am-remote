---
name: sync-contract
description: Bring android-controller in line with the shared API contract. Applies new entries from ../contract/CHANGELOG.md and open android-controller items from ../contract/conformance-report.md, verifies, and records progress. Use after the contract changed in the root window, or when the user asks to sync with the contract.
---

# /sync-contract (android-controller)

The root window owns the contract. This window only *applies* it, and never edits `../CLAUDE.md` or `../contract/`.

1. **Find what's pending.**
   - Read "Contract changes applied: C-NNN" in ./CLAUDE.md. Pending = entries in `../contract/CHANGELOG.md` newer than that id whose **Applies to** includes `app`.
   - Unticked `android-controller` or `both` items in `../contract/conformance-report.md` (if the file exists).
   - If nothing is pending, say so and stop.
2. **Show the list** to the user, with a line per item on what will change.
3. **Implement** inside this folder: models in `lib/core/models/`, api_client, ws_client, and the UI that reacts to new fields, events or errors. Add or update a test that round-trips the matching `../contract/samples.md` entries.
4. **Verify** with `/verify` and fix until it passes.
5. **Record progress** in this folder only:
   - Set the "Contract changes applied:" line in ./CLAUDE.md to the newest applied id.
   - In `docs/contract-requests.md`, tick the requests that an applied changelog entry lists under **Resolves**.
6. Suggest a commit: `feat(app): apply contract C-NNN` or `fix(app): contract conformance`.
