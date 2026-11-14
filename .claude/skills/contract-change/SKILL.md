---
name: contract-change
description: Change the shared am-remote API contract from the root window. Edits the contract in CLAUDE.md, contract/samples.md and contract/CHANGELOG.md, and settles open questions or requests from the projects. Use whenever an endpoint, JSON field, error code, WebSocket event, or pairing/discovery detail has to change, or to settle an open contract question. Never edits project code.
argument-hint: "<what should change> | requests"
---

# /contract-change $ARGUMENTS

Root window only. This changes the **contract**, not the projects. Each project applies the change later with `/sync-contract` in its own window.

## 1. Gather
- If the argument describes a change, start from that.
- If the argument is `requests` or empty, collect:
  - unticked items in `desktop-agent/docs/contract-requests.md` and `android-controller/docs/contract-requests.md`
  - "Open questions" in `contract/README.md`
  - the "Contract gaps" section of `contract/conformance-report.md` (if present)

  Then ask which ones to settle now (AskUserQuestion, multi-select).
- Read the current "API contract (v1)" section of `CLAUDE.md` and `contract/samples.md`.

## 2. Propose
Show the exact new contract text (before/after of the affected lines in `CLAUDE.md`) and the `samples.md` additions or edits. Classify the change:
- **Additive** (new optional field, endpoint, WebSocket event or error code): fine within v1.
- **Breaking** (rename, removal, type change, new required request field, changed status code): needs `/api/v2` and mDNS TXT `version=2`. Say so, and propose an additive alternative first.

Also say which projects must change: `agent`, `app` or both.

## 3. Confirm
AskUserQuestion: **Apply** / **Adjust** / **Cancel**. Edit nothing before "Apply".

## 4. Apply (root-owned files only)
1. `CLAUDE.md`: the contract section, plus the endpoint table's MVP column for new endpoints.
2. `contract/samples.md`: add or update examples. Keys are `<shape>_<variant>` in snake_case. Each sample is a `### key` heading followed by one `json` block. Then run `node .claude/scripts/check-setup.mjs contract/samples.md`.
3. `contract/CHANGELOG.md`: a new entry at the top, in the format at the top of that file, with the next `C-NNN` id. List the settled requests and questions under **Resolves** (for example `desktop-agent request "BAD_REQUEST code"`, `open question 1`).
4. `contract/README.md`: remove the settled open questions.

Leave the projects' `docs/contract-requests.md` files alone. Each project ticks its own requests when it syncs.

## 5. Hand off
Tell the user which windows to open and what to run: `/sync-contract` in desktop-agent and/or android-controller.
Suggest a commit: `docs(contract): C-NNN <title>`.
