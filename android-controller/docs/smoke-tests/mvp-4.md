# Smoke test — MVP 4: shuffle/repeat, Up Next and app search (android-controller)

Needs a human, the phone paired with the laptop, and the desktop-agent's **MVP 4** (`/player/modes`, `/queue`, `/queue/play`, `/app-search`, `/app-search/play`, and the `modes` WebSocket event). At the time of writing the agent did not have these routes yet, so **nothing below has been run against a real agent**.
The automated tests cover the logic and the screens with fakes (the 300 ms search debounce, refresh rules, error and "not available" states, request bodies, the 45 s wait). Whether Apple Music's window really follows is for you to see.

**Before you start**
- Agent running with its MVP 4, Apple Music open and signed in, with a few songs queued ("Playing Next") and something playing.
- App paired. From `android-controller/`: `flutter run`.
- Searching and playing move Apple Music's window on the laptop and change its queue. That is expected.

| # | Do | Expect |
|---|---|---|
| 1 | Open the **Player** tab. | Under the play / previous / next buttons there are two more: **shuffle** and **repeat**, both showing the laptop's real state. Under them is **Up Next** with a note "Shows about the next 20 songs" and the next songs (title, artist, length). |
| 2 | Tap **shuffle**. | The button changes only after the laptop answers (a second or two), and Apple Music's own shuffle button matches. Tap again: off again. |
| 3 | Tap **repeat** three times. | off → all → one → off. "One" uses a different icon (a 1 inside the arrows). Apple Music's repeat button matches each time. |
| 4 | Change shuffle and repeat **on the laptop**. | The phone follows within a few seconds (the agent checks about every 2 s). |
| 5 | Scroll the Up Next list. Pull the screen down. | It scrolls (the whole Player screen does; the volume row stays at the bottom). Pulling down reads the list again. |
| 6 | Tap the 3rd song in the list. | A small spinner shows on that row for about 1–2 s, then that song plays on the laptop, and the list moves on. The now-playing card changes to that song. |
| 7 | Skip a track on the laptop. | The list updates by itself. Turn **shuffle** on: the list is read again and reordered. |
| 8 | If the queue ends and Apple Music adds its own songs. | They appear under a small **AutoPlay** divider. Tapping one plays it, or shows "This song can't be played from the phone" if the agent can't. |
| 9 | Open the **Search** tab. Choose **Library**, type one of your artists (at least 2 letters), wait. | After about 4 s (a "Searching in Apple Music…" screen shows meanwhile) results appear grouped under headings such as **Songs**. There are no pictures. Tap a song: it plays on the laptop and the app jumps to the Player. |
| 10 | Choose **Catalog**, type an artist. | The same search runs again with Catalog. Results are grouped (for example Top Results, Songs, Albums). Tap an **album**: it plays from track 1 (allow about 5 s; the loading state stays visible meanwhile). |
| 11 | Tap a kind of result the agent can't play yet (for example an artist or a video). | "This kind of result can't be played from the phone yet". No crash, you stay on Search. |
| 12 | Change the text after a search and tap an old result quickly. | It plays the old result (the app repeats the search that produced it). |
| 13 | Close Apple Music on the laptop. | The Player shows "Open Apple Music on your laptop" (buttons and list included). Search shows the same. Reopen Apple Music and tap **Retry**: everything comes back, with the current shuffle and repeat. |
| 14 | If the agent has no MVP 4 (old agent): open Player and Search. | Shuffle and repeat are greyed with "Not available on this laptop", the list says "Queue isn't available on this laptop", search says "Search isn't available on this laptop yet". Play, pause, volume and Playlists still work. |
| 15 | Turn the phone's Wi-Fi off and on. | After it reconnects, shuffle, repeat and the list show the laptop's current state, including changes made meanwhile. |

**What the app does with the answers**
- Shuffle and repeat show what the agent answered or pushed, never a guess. While a change is on its way both buttons are off.
- The list is read when the Player opens, when the song (title or cover) changes, after a shuffle/repeat change, after a reconnect, and on pull-down. There is **no timer**.
- A tap on a list row sends its position and title; if the queue moved on meanwhile (`404`) you see "The queue changed" and the list is read again.
- A search result is played with the search that produced it (same text and Catalog/Library), plus its own section, position and title. If the search no longer finds it (`404`) the app searches again.
- All six new calls wait up to 45 s, because the agent drives Apple Music's window one call after another.
- The old iTunes-based `GET /search` and `POST /play/track` are no longer used by any screen (their client code is still there).

**Still to check on a device:** all of the above. Layout was only checked in tests on a 411 × 890 and a 360 × 640 screen: on a short phone, shuffle and repeat and the list are a scroll below the transport buttons, which stay on screen.

**Not built:** playing a single track from inside a playlist; Apple Music's search suggestions; artwork in search results (the contract has none for these).
