---
name: build-mvp
description: Build one android-controller roadmap phase (MVP 0-4) by following .claude/tasks/mvp-<n>.md. Use only when the user asks to build or implement an MVP phase.
argument-hint: "[0-4]"
disable-model-invocation: true
---

# /build-mvp $ARGUMENTS (android-controller)

1. **Phase.** Use the number 0-4 from the arguments. If there isn't one, take the first unticked MVP under "Current status" in ./CLAUDE.md, and say which one you picked.
2. **Read** `.claude/tasks/_workflow-rules.md`, then `.claude/tasks/mvp-<n>.md`.
3. **Follow** the five steps in the rules file (plan → build → check → contract review → wrap-up), using the task file's Gate, Build, Tests, Done-when and Smoke-test sections.
4. **Finish** with the report format from the rules file. Don't commit.
