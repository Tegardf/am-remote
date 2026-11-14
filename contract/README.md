# contract/

Example JSON for the am-remote v1 API. The spec itself lives in the root [`CLAUDE.md`](../CLAUDE.md) ("API contract (v1)").
That text is the source of truth. This folder holds concrete examples of it, so both projects can test against the same JSON.

## `samples.md`
One sample per wire shape, for example `player_state_playing`, `volume_request_level`, `error_unauthorized` or `ws_state`. Each sample is a `### <key>` heading followed by exactly one fenced `json` block. It's Markdown, so people and Claude can read it directly, and tests can still parse it.

Both test suites split the file on `### ` headings, take the `json` block under each, and round-trip every sample they model through their real types:

| Project | How |
|---|---|
| `desktop-agent` | `include_str!("../../contract/samples.md")` in `tests/contract_samples.rs`, then `serde_json::from_str::<T>()` and back to a value, and assert the result equals the sample |
| `android-controller` | `File('../contract/samples.md')` in `test/contract_samples_test.dart` (the working directory is the package root), then `T.fromJson()` and `toJson()`, and assert equality |

`node .claude/scripts/check-setup.mjs` (root) checks that every block parses and every key is snake_case.

If a sample stops round-tripping, one side has drifted from the contract.

HTTP status codes aren't in the samples: `204` for empty success, and the status for each error code, are in the contract table.

## Files here
| File | Written by | Read by |
|---|---|---|
| `samples.md` | root window (`/contract-change`) | both projects' tests |
| `CHANGELOG.md` | root window (`/contract-change`) | project windows (`/sync-contract`, `/build-mvp`) |
| `conformance-report.md` | root window (`/contract-check`) | project windows (`/sync-contract`, `/build-mvp`) |

The projects only *read* this folder. When they need a contract change, they log it in their own `docs/contract-requests.md`.

## Changing the contract
Only from the root window, with `/contract-change <what>` (or `/contract-change requests` to work through the projects' requests). It walks through this order:

1. Root `CLAUDE.md`: the contract text
2. `contract/samples.md`
3. `contract/CHANGELOG.md`: a new `C-NNN` entry
4. Then, in each project window, `/sync-contract` applies it to the code.

## Open questions
The contract doesn't answer these yet. Settle each one with `/contract-change`, then delete it from this list.

1. **Validation errors have no code.** A malformed JSON body, a missing `code` in `/pair`, or an out-of-range `level` all need a response, but the error list has no `400`. axum's default rejection is a plain-text 400/422, which breaks the error-body rule. Suggestion: add `BAD_REQUEST` (400).
2. **Empty PlayerState.** When nothing is loaded, are `title`/`artist`/`album` `null` or `""`? Are `duration_ms`/`position_ms` `0` or `null`?
3. **Volume before the first playback.** Windows only creates Apple Music's audio session after it has played something. What do `GET/POST /volume` return until then? `409 APPLE_MUSIC_NOT_RUNNING` would be misleading.
4. **`artwork_url` vs "no internet traffic".** `SearchResult.artwork_url` is an internet HTTPS URL, but `android-controller/CLAUDE.md` says the app makes no internet traffic. Either allow HTTPS image loads, or have the agent proxy search artwork.
5. **`/status` before pairing.** As written, `/status` needs a token. Should it be public, so the app can check reachability before pairing? The mDNS TXT record already gives the version.
