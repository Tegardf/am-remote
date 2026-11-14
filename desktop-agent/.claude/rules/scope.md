# Working in the desktop-agent window

- **Edit only inside this folder.** `../CLAUDE.md` (the roadmap and API contract) and `../contract/` belong to the root window. Read them, but never edit them. `../android-controller/` is a different project.
- **Contract problems go to a request file, not into the contract.** If the contract is silent, contradictory, or missing something you need, pick the least surprising behaviour and keep it in one place (a const or a single function). Then append it to `docs/contract-requests.md` in this format:
  `- [ ] <what the contract should say> - why: <reason> - default used: <what the code does now> (MVP <n>, YYYY-MM-DD)`
  The root window settles these with `/contract-change`, and this window applies the result with `/sync-contract`.
- **Real-machine facts** (the Apple Music AUMID, process name, UI Automation selectors): find them with a small probe in `examples/` and keep each in one const commented `// verified: <how, date>` or `// UNVERIFIED`. Never guess silently.
- **Done means `/verify` is green:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` and `cargo build`. Tests must not need Apple Music, the tray, a window, or the network beyond localhost; use the fake `MediaBackend`.
- **Long-running processes:** `cargo run` starts a server and tray. Start it only for a short smoke check with a timeout, and stop it afterwards.
- **Status:** tick `- [x] MVP <n>` in ./CLAUDE.md only when the whole phase works and checks pass. Keep "Contract changes applied:" up to date (see `/sync-contract`).
- **Commits** (only when the user asks): `feat(agent): …`, `fix(agent): …`, `test(agent): …`, `docs(agent): …`. Never commit `target/` or anything from `%APPDATA%\am-remote`.
