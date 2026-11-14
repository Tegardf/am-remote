# Spike: link-playback

**Decision:** MIXED · **Confidence:** medium · **Date:** 2026-11-14 · **Apple Music version:** 1.1792.39942.0 (Microsoft Store, x64, English UI)

> **Correction, found while building MVP 3 (2026-11-14).** The first version of this report said a single song can be played by selecting its row and pressing Enter. That test was done on a row of the **play-queue pane**, which at that moment held the album I had just played, not on a row of the album page. On the real album page, rows are named `"Track 6 Jatuh Suka 3 minutes, 55 seconds"`; neither Invoke, the row's legacy default action ("Double Click"), nor select + Enter plays them. What works is each row's own **"..." menu**, which has an entry **`Play “<title>”`**. The decision (MIXED) is unchanged; the song steps below are the corrected ones, and MVP 3 implements them.

Confidence is medium because everything was observed on one machine, one app version and one UI language, and the working route depends on UI Automation. Each step below was observed to work, not inferred.

## Question
When the agent opens a `music.apple.com` link (or a URI scheme the Apple Music app registers), does the Apple Music app for Windows open it **and start playing** a catalog song, an album and a playlist, without anyone clicking? The answer decides MVP 3: link-based `POST /play/track` and `/play/playlist`, or UI Automation.

## What we observed
Probe programs are in `examples/` (`spike_link_playback`, `spike_link_page`, `spike_link_click`, `spike_playlists`).

**1. The installed app** (`Get-AppxPackage`, `AppxManifest.xml`, read-only):
- Package family `AppleInc.AppleMusicWin_nzyj5cx40ttqa`, application ids `App` (`AppleMusic.exe`), `LibraryServer` (`AMPLibraryAgent.exe`), `SharedHelper`, `AppleInc.Music.Defaults`.
- URL schemes registered (all launched as `AppleMusic.exe /url "%1"`): `music`, `musics`, `itms`, `itmss`, `itunes`, `itunesradio`, `itpc`, `daap` and some legacy `itunes.assocprotocol.*`.
- Web-to-app link hosts (`uap3:AppUriHandler`): `music.apple.com`, `buy.itunes.apple.com`.
- Execution alias `AppleMusic.exe`. File associations launch with `/play "%1"`.

**2. Do links autoplay? No.** `spike_link_playback` ran 5 link forms against a song, an album and a public playlist (15 trials), polling SMTC for 10 s each:

| Form | song | album | playlist |
|---|---|---|---|
| `rundll32 url.dll,FileProtocolHandler music://…` (ShellExecute) | no playback | no playback | no playback |
| same with `musics://…` | no playback | no playback | no playback |
| same with `itmss://…` | no playback | no playback | no playback |
| `AppleMusic.exe /url https://…` | no playback | no playback | no playback |
| `AppleMusic.exe /url music://…` | no playback | no playback | no playback |

Every trial: launch succeeded, track unchanged, nothing playing after 10 s. The plain `https://` form through ShellExecute was not tried (it can open a browser tab, and the research found nothing suggesting it behaves differently). **LINK is ruled out.**

**3. Does the link at least navigate the app? Yes, for `music://`.** `spike_link_page` opened `music://music.apple.com/id/album/<slug>/<id>` and read the main window through UI Automation: the album page was showing, with the artist link, a **Play** button (`AutomationId` `PlayButtonElement`), a **Shuffle** button (`ShuffleButtonElement`), and the track list.

**4. A UI click then plays it. Yes.**
- **Album:** invoking `PlayButtonElement` (UI Automation `InvokePattern`, no mouse or keyboard) started the album in ~0.4 s (first track, `Tujuh Belas`).
- **Playlist:** the same on a public playlist page, playing in ~1.2 s.
- **Single song:** the song link (`?i=<trackId>`) opens the **album** page. Its track rows are `ListViewItem` elements named `"Track <n> <title> <duration>"` (the play-queue pane on the right lists rows named `"<title> <artist> — <album> <duration>"`: do not mix them up). On a page row, **Invoke**, the legacy default action (**"Double Click"**) and **select + Enter** all did **nothing**. Each row contains a button (its "More" / "..." menu, invokable) whose menu lists `Add to Library`, `Play “Jatuh Suka”`, `Play Next`, `Add to Queue`, `Create Station`, ... Invoking the entry `Play “Jatuh Suka”` (an `InvokePattern` on a `MenuItem`) plays exactly that song. The page's track list is virtualized: when the window is short, the rows below the fold do not exist in the UI tree until the page is scrolled (`ScrollPattern` on the page's `ListView`).

**5. The user's own library playlists are readable.** Selecting the sidebar item "Playlists" (`Sidebar_Header_Playlists`) expands the list. Each library playlist is a `NavigationViewItem` whose `AutomationId` has the form `DBID:0x…-PID:0x…-PPID:0x…-PIPID:0x…-IKIND:ePlaylist`, and whose `Name` is the playlist name. There is also `Sidebar_AllPlaylists`. These ids look like Apple's persistent library ids, so they are a good candidate for the contract's opaque playlist `id`. Whether they survive an Apple Music restart or an app update was **not** verified.

**6. Other facts.** UI element names are localized ("Play", "Playlists") but `AutomationId`s are not. While the app is paused or just navigated, SMTC briefly reports the title `Connecting…` right after playback starts, so playback detection must ignore it.

## What research says
A subagent searched the web; these are hypotheses that the probes then tested.
- Users report `music://` opens an album page and "nothing is played" ([audio-follow README](https://github.com/dustinshanehall/audio-follow)). Matches observation 3. Confidence: reported by users.
- The same project plays a song with UI Automation plus simulated input (double-click on the row) and warns it may break after app updates. Matches observation 4. Confidence: reported by users.
- On macOS `music://music.apple.com/...` "takes you directly to the playlist" without autoplay ([MacMost](https://macmost.com/create-finder-shortcuts-to-apple-music-playlists.html), [Apple Support thread](https://discussions.apple.com/thread/251588673)). Confidence: reported by users.
- Users report that `https://music.apple.com` links open in the browser first ([Apple Support thread](https://discussions.apple.com/thread/255578039), not fetched; search snippet only). Our manifest does declare `music.apple.com` as an app link host, but this was not tested. Confidence: low.
- iTunes for Windows registered `itms`/`itmss` as `iTunes.exe /url "%1"` ([exploit-db 8934](https://exploit-db.com/exploits/8934)); the Store app's manifest shows the same pattern with `AppleMusic.exe`. Confirmed by the manifest here.
- No source documents any autoplay link form, and none documents a non-UI-Automation way to pick a specific track. SMTC cannot choose a track (see `../CLAUDE.md`).

## Recommendation for MVP 3
Use the **MIXED** approach: a link (or the sidebar) navigates, then UI Automation presses Play. Isolate it in `media/uia.rs` behind the `uia` cargo feature, as planned. Match elements by **AutomationId**, never by localized name.

**`GET /search`** is independent of this spike: the iTunes Search API, as the task describes.

**`POST /play/track { url }`** (a search result's `trackViewUrl`):
1. Accept only `https://music.apple.com/…`; reject anything else with the validation error.
2. Read the track id from the `i` query parameter. Look the track up with `https://itunes.apple.com/lookup?id=<i>&country=<storefront in the URL path>` to get `trackName` and `artistName` (the URL alone has no title). Offline: `INTERNAL` with a clear message.
3. Open the link as `music://music.apple.com/…` (the same URL with the scheme swapped) with ShellExecute. Apple Music must already be running (`409 APPLE_MUSIC_NOT_RUNNING` otherwise; whether the link would launch a closed app was not tested).
4. Wait until the album page is showing: a visible text element named exactly like the album (`collectionName` from the lookup) below the transport bar. (Playlist pages have the header element `ContainerDetailHeaderLockupRegularContent` named like the playlist; album pages do not, found with `examples/uia_named.rs`.)
5. Find the page row `"Track <n> <title> <duration>"` for the song (page rows only, never the queue pane); scroll the page down until it exists.
6. Invoke the button inside that row, then invoke the menu entry named `Play “<title>”` (matched by the quoted title, not by the word "Play").
7. Confirm through SMTC (up to ~6 s, ignoring the transient title `Connecting…`) that the title is `trackName`. If not, `501 NOT_SUPPORTED` "Apple Music did not start the song".
8. A link that is not a song (no `i`) is a `400 BAD_REQUEST`: `/search` only returns song links.

**`GET /playlists`:** if no playlist is listed yet, select `Sidebar_Header_Playlists` (**selecting it toggles the list**: selecting it while open collapses it, so only select it when the list is not showing), wait for the list, read every `NavigationViewItem` whose `AutomationId` ends in `IKIND:ePlaylist` → `{ id: <AutomationId>, name: <Name> }`. Skip `Sidebar_AllPlaylists`. Playlist folders (`ChildrenFlyout`) were not examined.

**`POST /play/playlist { id }`:** select `Sidebar_Header_Playlists`, find the `NavigationViewItem` with `AutomationId == id`, `Select()` it, wait for the page, invoke `PlayButtonElement`, then confirm through SMTC that status is `playing`. Unknown id: `404 NOT_FOUND` (a playlist that is no longer in the library is not a malformed request).

**Failure modes → contract errors:** any element not found or any step timing out → `501 NOT_SUPPORTED` with a message saying which step failed; no hangs (timeouts on every UI Automation call); Apple Music not running → `409 APPLE_MUSIC_NOT_RUNNING`; no network for lookup/search → `INTERNAL`.

**Side effects the user will see:** the Apple Music window navigates to the album or playlist page, and the play queue changes. No key or mouse input is simulated: everything is done through the accessibility interface, so the Apple Music window does not need to be in the foreground (a minimized window is restored).

## Fragility and how to detect breakage
- `AutomationId`s (`PlayButtonElement`, `Sidebar_Header_Playlists`, the `DBID:…IKIND:ePlaylist` pattern) and the row naming are Apple's internal choices and can change in any update. The package version here is 1.1792.39942.0; keep it in one const with `// verified: <date>`.
- Add a startup self-check behind the `uia` feature: find `PlayButtonElement` on an album page and `Sidebar_Header_Playlists`; log a warning with the app version if they are missing, and answer the playlist and play endpoints with `501 NOT_SUPPORTED` instead of trying.
- Keep the probes in `examples/` (`spike_link_click`, `spike_playlists`) so a broken update can be diagnosed in minutes.
- Row matching by name depends on the `"<title> <artist> — <album> <duration>"` pattern and on the English UI; for another language the pattern may differ.

## Manual steps still needed
None to decide. Things **not** verified, to check during the MVP 3 smoke test:
- That playlist ids (`DBID…`) are the same after restarting Apple Music.
- What happens when Apple Music is closed (does `music://` launch it?).
- A non-English UI, a minimized or hidden window, playlist folders, and a very large library (is the sidebar list complete without scrolling?).
- The plain `https://` form through ShellExecute (not needed for the recommendation).
