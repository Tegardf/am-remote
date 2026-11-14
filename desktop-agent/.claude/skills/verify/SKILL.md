---
name: verify
description: Run the desktop-agent checks (cargo fmt --check, clippy with warnings as errors, tests, build) and report pass/fail with the first errors. Use before saying a change is done, before committing, or when the user asks to check the build.
---

# /verify (desktop-agent)

If there is no `Cargo.toml` yet, say the crate isn't scaffolded (`/build-mvp 0` does that) and stop.
If `cargo` is missing, say so and point to README.md "What to install". Don't install anything.

Run these from this folder, in order, and continue after a failure so the report is complete:
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. `cargo build`

Report a table with one row per check: check, ✅/❌, one-line summary. Then show the first ~30 lines of each failure.
Don't fix anything unless asked. If asked, fix the code (not by loosening the checks) and re-run.
