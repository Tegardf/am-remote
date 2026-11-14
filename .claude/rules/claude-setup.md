---
paths:
  - ".claude/**"
  - "desktop-agent/.claude/**"
  - "android-controller/.claude/**"
---

# Maintaining the Claude Code setup

The root window maintains three Claude setups. Each project window is its own Claude project, so its `.claude/` must be self-contained: settings, skills, tasks, rules, hooks. The only things outside it that a project uses are `../CLAUDE.md` (auto-loaded) and `../contract/` (read-only).

## Keep session start light
Loaded at **every** session start: all `CLAUDE.md` files from the folder upward, rules **without** `paths:`, skill names and descriptions, and memory. Keep those short:
- Detailed instructions go in `.claude/tasks/*.md`, which load only when a skill opens them.
- Rules get `paths:` unless they really apply to every edit. Only `scope.md` is unscoped, and it stays under ~15 lines.
- A skill description is one sentence, and the skill body is a short launcher.
- Don't copy the contract into task or rule files. Point at `../CLAUDE.md` and `../contract/samples.md` instead.

## Task files (`<project>/.claude/tasks/`)
- Plain Markdown. MVP tasks use the sections **Goal**, **Gate**, **Build** (checklist), **Tests (required)**, **Done when**, **Smoke test**, **Next**, and start with "Follow `_workflow-rules.md`".
- `_workflow-rules.md` holds what's shared: ownership, the five steps, the review and skeptic prompts, the report format. Keep the desktop-agent and android-controller copies parallel, and when you change one section, change the other.
- Paths in project tasks are relative to the project folder (`./CLAUDE.md`, `../contract/samples.md`). Root tasks use root-relative paths.
- Every prompt that hands work to a subagent says whether the subagent may edit.

## Other parts
- **Settings** must stay JSON (`settings.json`); it's the only non-Markdown config.
- **Hooks** are Node scripts that exit 0 silently when their tool is missing.
- **Samples:** `contract/samples.md` is `### <snake_case_key>` followed by one `json` block. Only `/contract-change` edits it.

## After any change
Run `node .claude/scripts/check-setup.mjs` (the root hook runs it on each edit). Then update the command tables in the affected `README.md` files.
