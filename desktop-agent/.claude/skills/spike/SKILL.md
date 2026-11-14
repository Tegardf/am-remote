---
name: spike
description: Run a desktop-agent technical spike (link-playback or uia-queue) by following .claude/tasks/spike-<name>.md, and record the decision in docs/spikes/. Use only when the user asks for a spike.
argument-hint: "link-playback | uia-queue [music.apple.com url]"
disable-model-invocation: true
---

# /spike $ARGUMENTS

1. **Name.** If it isn't `link-playback` or `uia-queue`, ask which one (AskUserQuestion). An optional second argument is a URL to test.
2. **Pre-flight.** Apple Music must be running and signed in (`Get-Process | Where-Object { $_.ProcessName -like '*Music*' }`). If it isn't, ask the user to open it and play a song once. For `link-playback`, warn that music will start playing on the laptop.
3. **Read** `.claude/tasks/_workflow-rules.md` (the Ownership section) and `.claude/tasks/spike-<name>.md`, then follow the task's steps.
4. **Report** the decision, confidence, report path and manual steps. If the decision is `NEEDS-MANUAL-RUN`, walk the user through the steps and update the report with what they saw. Don't commit.
