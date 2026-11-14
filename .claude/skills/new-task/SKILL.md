---
name: new-task
description: Create or update a Markdown task, rule, skill or hook for desktop-agent, android-controller or the root, from the root maintenance window, following this repo's conventions. Use when the user wants a new task or automation for a project, or wants to change an existing one.
argument-hint: "<desktop-agent|android-controller|root> <what it should do>"
---

# /new-task $ARGUMENTS

The root window maintains every project's Claude setup. The project windows **use** it, so write it here but don't run it here.

1. **Understand the request.** Ask what it's for, when it runs, and what "done" looks like if that's unclear. Read the target's `.claude/tasks/README.md`, `_workflow-rules.md` and an existing task first, and reuse their shape.
2. **Pick the lightest form** (see `.claude/rules/claude-setup.md`):
   | Form | Loaded | Use for |
   |---|---|---|
   | Task `.claude/tasks/<name>.md` | only when run | step-by-step work: what to build, gate, tests, done-when, smoke test |
   | Rule `.claude/rules/<name>.md` with `paths:` | only when matching files are touched | guidance while editing certain files |
   | Skill `.claude/skills/<name>/SKILL.md` | description only, at start | a short launcher: "read task X and follow it" |
   | Hook (`settings.json` + `.claude/hooks/*.mjs`) | on an event | something that must always happen automatically |
3. **Write it.** A new task needs a row in that project's `.claude/tasks/README.md`. Give a new skill a one-sentence description, and `disable-model-invocation: true` if it does heavy work.
4. **Validate:** `node .claude/scripts/check-setup.mjs`.
5. **Document:** update the command table in the project's `README.md` and, if the command is user-facing, in the root `README.md`.
6. Suggest a commit: `chore(claude): <project> <what>`.
