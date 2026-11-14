# Spike — UI Automation queue (desktop-agent)
Follow the **Ownership** rules in [_workflow-rules.md](_workflow-rules.md). The five MVP steps don't apply; follow the steps below.

## Question
Can the agent read the **current playlist name** (and ideally the **Up Next** queue and the **sidebar playlist list**) from the Apple Music for Windows window through UI Automation, robustly, i.e. by AutomationId / Name / ControlType rather than screen coordinates?
The answer decides whether `PlayerState.playlist_name` is filled in MVP 2, and whether `GET /playlists` uses UIA in MVP 3.

## Ground rules
UI Automation read-only access only. Don't click or type unless a step says so. No process injection, no DRM, no Apple private APIs. Put timeouts on every UIA call.

## Steps
1. **Tree dump (PowerShell 5.1, read-only).**
   - Load the UIA assemblies: `Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes`.
   - Find the Apple Music main window (`Get-Process` → `MainWindowHandle` → `AutomationElement.FromHandle`).
   - Dump 4 levels of `ControlType`, `Name`, `AutomationId`, `ClassName` and `FrameworkId` to `docs/spikes/uia-queue-tree.txt`. Note which UI framework it is.
2. **Research (web).** UIA/accessibility quality of the Windows Apple Music app, how others read Up Next or playlists, and the API of the Rust `uiautomation` crate. Cite a URL for every claim.
3. **Probe.**
   - Where: if `Cargo.toml` exists and cargo works, write `examples/spike_uia_queue.rs` with `uiautomation` in `[dev-dependencies]`. Otherwise extend the PowerShell script as `docs/spikes/uia-queue.probe.ps1`.
   - Try to locate (a) the current playlist/album context name, (b) the Up Next list and its items, (c) the sidebar playlists.
   - For each one, print the selector path you used and the text you extracted.
   - If a panel has to be open (e.g. Up Next), report what's visible with it closed, then list the manual step.
4. **Decide from observed results only.** If Apple Music wasn't running or the probe didn't run, the decision is `NEEDS-MANUAL-RUN`.

## Output: docs/spikes/uia-queue.md
```
# Spike: uia-queue
**Decision:** GO | PARTIAL | NO-GO | NEEDS-MANUAL-RUN · **Confidence:** high|medium|low · **Date:** YYYY-MM-DD · **Apple Music version:** x
## Question
## What we observed        (selector paths and extracted text; see uia-queue-tree.txt)
## What research says      (links)
## Recommendation          (MVP 2 playlist_name; MVP 3 GET /playlists: selectors, timeouts, fallback to null / 501)
## Fragility and how to detect breakage
## Manual steps still needed
```
- **GO**: stable selectors for `playlist_name`.
- **PARTIAL**: only some data is reachable, or a panel must be open.
- **NO-GO**: `playlist_name` stays `null`.

## Report
The decision, confidence, report path, and any manual steps. Suggest a commit: `docs(agent): spike uia-queue - <decision>`.
