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
None open. Everything raised so far is settled in C-001 to C-005 (see [CHANGELOG.md](CHANGELOG.md)).

When something in the contract turns out to be unclear, add it here (numbered) or log it in the project's `docs/contract-requests.md`, then settle it with `/contract-change`.
