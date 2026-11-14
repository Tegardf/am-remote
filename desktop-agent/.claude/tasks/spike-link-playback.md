# Spike — link playback (desktop-agent)
Follow the **Ownership** rules in [_workflow-rules.md](_workflow-rules.md). The five MVP steps don't apply; follow the steps below.

## Question
When the agent opens a `music.apple.com` link (or a URI scheme the Apple Music app registers), does the Apple Music app for Windows open it **and start playing** a catalog song, an album and a playlist, without anyone clicking?
The answer decides MVP 3: link-based `POST /play/track` and `/play/playlist`, or UI Automation.

## Ground rules
Use public OS APIs only (ShellExecute / `Launcher`, SMTC, read-only registry and package manifest). No process injection, no reading Apple Music's data files, no DRM, no Apple private APIs. Put a timeout on everything, and leave no process running.
**This spike starts music on the laptop.** Tell the user before running it.

## Steps
1. **Inspect the installed app (read-only, PowerShell).** Run `Get-AppxPackage *AppleMusic*` to get the version, PackageFamilyName and InstallLocation. From `AppxManifest.xml`, list every `uap:Protocol` name, the `uap3:AppUriHandler` hosts, and the Application Id. Derive the AUMID.
2. **Research (web).** Which schemes the Windows app handles (`music`, `musics`, `itms`, `itmss`, `applemusic`, the https app URI handler), whether any link form autoplays, and how the Windows app differs from iTunes/macOS. Cite a URL for every claim. Run this as a subagent in parallel with step 1 if you like.
3. **Probe.**
   - Where: if `Cargo.toml` exists and cargo works, write `examples/spike_link_playback.rs` (crates go in `[dev-dependencies]`). Otherwise write `docs/spikes/link-playback.probe.ps1`.
   - Test URLs: a song and an album from `https://itunes.apple.com/search?term=<popular artist>&entity=song&limit=1` (`trackViewUrl`, `collectionViewUrl`), a public playlist URL, and any URL the user supplied.
   - For each URL × each link form from steps 1-2: record the SMTC state, open the link, poll SMTC for up to 10 s, and print `{ url, form, app_opened, track_changed, playing, elapsed_ms }`. Pause through SMTC at the end.
   - If Apple Music isn't running or the probe can't run, write exact manual steps instead.
4. **Decide from observed results only.** Web claims are hypotheses until the probe confirms them. If the probe didn't run, the decision is `NEEDS-MANUAL-RUN`.

## Output: docs/spikes/link-playback.md
```
# Spike: link-playback
**Decision:** LINK | MIXED | UIA | NEEDS-MANUAL-RUN · **Confidence:** high|medium|low · **Date:** YYYY-MM-DD · **Apple Music version:** x
## Question
## What we observed        (probe output excerpts)
## What research says      (links)
## Recommendation for MVP 3 (URL form or selectors to use, failure modes → which contract error code)
## Fragility and how to detect breakage
## Manual steps still needed
```
- **LINK**: links reliably start playback.
- **MIXED**: a link opens the item, but needs an extra play command or UIA click.
- **UIA**: links don't work.

## Report
The decision, confidence, report path, and any manual steps. Suggest a commit: `docs(agent): spike link-playback - <decision>`.
