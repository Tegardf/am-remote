# Smoke test — MVP 3: playlists and search (android-controller)

Needs a human, the phone paired with the laptop, and the desktop-agent's **MVP 3** (`GET /playlists`, `POST /play/playlist`, `GET /search`, `POST /play/track`). Those need the agent's link-playback spike first.
The automated tests cover the logic with fakes (including the 300 ms search debounce in fake time). The steps marked ✅ were also seen on the emulator; the others need the agent's MVP 3 and have not been run against a real agent.

**Before you start**
- Agent running with its MVP 3 endpoints, Apple Music open and signed in.
- App paired. From `android-controller/`: `flutter run`.

| # | Do | Expect |
|---|---|---|
| 1 ✅ | Open the app. | A bottom bar with **Player · Playlists · Search**. Player is first. |
| 2 | Open the **Search** tab, type an artist (at least 2 characters), then pause. | After about 0.3 s the results appear with a small artwork picture, the title and "artist · album". One character shows nothing. |
| 3 | Tap a result. | It starts playing on the laptop and the app switches to the **Player** tab, which shows it. |
| 4 ✅ | Open the **Playlists** tab (it can take several seconds the first time). | Your playlists are listed (the agent reads them only when you first open the tab). Pull down to reload. |
| 5 ✅ | Tap a playlist. | It plays on the laptop and the app switches to the **Player** tab. |
| 6 | If the agent cannot read playlists (`501 NOT_SUPPORTED`). | The Playlists tab says "Playlists aren't available on this laptop yet": no error, no crash. Search and the Player still work. |
| 7 | Type a search, switch to Player and back to Search. | The text and the results are still there, and nothing is searched again. |
| 8 | Close Apple Music, then search or open Playlists. | "Open Apple Music on your laptop" with **Retry**; it works again after you open Apple Music and tap Retry. |
| 9 | Turn the phone's Wi-Fi off and search. | "Couldn't search. Check the connection to your laptop." with **Retry**. Back online and Retry: it works. |
| 10 | Search for something that doesn't exist (for example `zzzzqqqq`). | `No results for "zzzzqqqq"`. |
| 11 | Tap a result whose artwork fails to load, or has none. | A grey note icon instead of a picture, no error. |

**What the app does with the answers**
- Only `https://` artwork is loaded. Anything else shows the placeholder. The only plain HTTP the app sends goes to your laptop.
- `limit` is always 25 and the search text is URL-encoded.
- A result is played with its own `url` exactly as the agent gave it.

**Seen on a real phone against the real agent (2026-11-14):** the owner confirmed the Playlists tab works with the agent's MVP 3 and matches the contract (steps 4–5). The first try failed with "Couldn't load your playlists" because the app gave up after 10 s while the agent may take up to 35 s to drive Apple Music's window; the app now waits up to 45 s for these calls (the agent's worst case is about 41 s) and shows the reason under the error. Steps 2, 3 and 6–11 (search, errors, offline) were not reported on and are still worth a run.

**Seen on the emulator (2026-11-14, agent not running):** the three tabs and the bottom bar render; Playlists shows "Couldn't load your playlists" with Retry, and Search shows its hint. The agent had no MVP 3 endpoints at that point.

**Not built:** playing a single track from a playlist (the contract only has "play this playlist"). The now-playing card already shows the playlist name when the agent sends one (MVP 2).
