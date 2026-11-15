# Spike — MVP 4 feasibility: shuffle/repeat, queue, app search (desktop-agent)
Follow the **Ownership** rules in [_workflow-rules.md](_workflow-rules.md). The five MVP steps don't apply; follow the steps below.

## Why
MVP 4 needs three things the current contract doesn't have. Each depends on what Apple Music for Windows really exposes. This spike answers them **before** the contract change C-006, so the contract is written once, from facts.

## Ground rules
Public OS APIs only: SMTC, UI Automation patterns (Invoke, Toggle, Value, SelectionItem, ScrollItem), read-only registry and package manifest. No simulated keyboard or mouse input, no process injection, no DRM, no Apple private APIs. Timeouts on every call; leave no process running. Probes may change what plays and what the Apple Music window shows: tell the user first, and restore shuffle/repeat to their original state at the end.

## Questions and probes
Write one probe per question in `examples/` (`spike_mvp4_shuffle.rs`, `spike_mvp4_queue.rs`, `spike_mvp4_search.rs`), add crates as `[dev-dependencies]`, and run each with Apple Music open and playing.

**A. Shuffle and repeat**
1. Read SMTC `PlaybackInfo.IsShuffleActive` and `AutoRepeatMode`, and `Controls.IsShuffleEnabled` / `IsRepeatEnabled`. Are they reported for Apple Music?
2. Call `TryChangeShuffleActiveAsync(true/false)` and `TryChangeAutoRepeatModeAsync(None/List/Track)`. Accepted? Does the app's own button state change, and does the next track follow it?
3. If SMTC can't do it: find the shuffle and repeat buttons in the UIA tree (AutomationId/Name/ControlType, TogglePattern state). Can they be read and toggled? How many repeat states (off/all/one) can be reached?
4. Do changes made in the app raise `PlaybackInfoChanged` (needed to push them to the phone)?

**B. Queue (Up Next)**
1. Find the play-queue pane (seen on the right during the link-playback spike). Is it present when closed, or must it be opened? Can it be opened through UIA?
2. Read every row: title, artist, duration if shown, which row is current, and how many rows before scrolling (virtualised list? `ScrollItemPattern`/`ItemContainerPattern`).
3. Play a row: `InvokePattern`, `SelectionItemPattern` plus a play command, or the row's context menu "Play". Does it start that song and keep the rest of the queue?
4. What stays stable as an id for a row across reads (position, name, AutomationId)? How fast is a full read (ms) for 20 and 100 rows?
5. Does anything signal a queue change, or must it be polled? What does Shuffle do to the order?

**C. Search through the app**
1. Find the search box. Can its text be set with `ValuePattern.SetValue` (no typing)? How is the search started (Invoke on a button, `SetValue` alone, or a suggestion list)?
2. Is "Apple Music" (catalog) vs "Library" selectable through UIA?
3. Read the results page: sections (Top results, Songs, Albums, Artists, Playlists), and for each item title, artist, type, and whether artwork or a link is reachable. How long until results appear?
4. Play a result: song, album and playlist. Which pattern works, and does it play without further steps?
5. Side effects: does the laptop window jump to the search page? Does it lose the user's place? Can it go back afterwards?

## Output: docs/spikes/mvp4.md
```
# Spike: mvp4
**Decision:** A shuffle/repeat: SMTC | UIA | NO · B queue: GO | PARTIAL | NO-GO · C app search: GO | PARTIAL | NO-GO · **Confidence:** … · **Date:** … · **Apple Music version:** …
## A. Shuffle and repeat — observed, how to read, how to set, events
## B. Queue — selectors, row fields, current row, play a row, id stability, timing, change detection
## C. App search — selectors, how to submit, result sections and fields, play a result, timing, side effects
## Proposed contract for C-006 (endpoints, fields, events, error codes) — from what was observed only
## Fragility and how to detect breakage
## Manual steps still needed
```
Decide from what the probes observed. If a probe could not run, that part is `NEEDS-MANUAL-RUN`.

## Report
The three decisions, the proposed C-006 contract, and the report path. Then the user runs `/contract-change` in the **root** window with that proposal. Suggest a commit: `docs(agent): spike mvp4 - <decisions>`.
