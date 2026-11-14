---
name: contract-check
description: Read-only cross-check of desktop-agent and android-controller against the API contract (follows .claude/tasks/contract-check.md) that writes contract/conformance-report.md. Use only when the user asks for a contract or conformance check.
argument-hint: "[mvp 0-3]"
disable-model-invocation: true
---

# /contract-check $ARGUMENTS

Read `.claude/tasks/contract-check.md` and follow it. It uses 2-5 read-only subagents and writes only `contract/conformance-report.md`. Never fix project code from this window.
