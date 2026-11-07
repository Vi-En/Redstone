# AGENTS.md

Open-source Minecraft server host app. Runs fully on the user's device: no cloud, no account, no telemetry.
Stack: Rust (edition 2021), Tauri 2 + Svelte UI, Axum, SQLite, Iroh (remote control only).
Feature specs: `docs/specs/`. Path-specific rules: `.github/instructions/`.

## Commands (all must pass before every commit)
- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Test: `cargo test --workspace`
- UI: `npm --prefix ui run check && npm --prefix ui run test`
- Run: `cargo tauri dev`

## Working rules
1. Think first. State assumptions in the PR description. If the task is ambiguous, pick the simplest reading, say so, and proceed. In interactive chat, ask instead.
2. Simplest correct code. No speculative features, config options, or abstractions.
3. Surgical changes. Every changed line traces to the task. Match existing style. Don't refactor or reformat unrelated code. Remove only what your change made unused.
4. Tests first. Failing test, then fix. Bug fix = reproducing test. Refactor = tests green before and after.
5. Optimize only when a benchmark shows a problem.
6. Fail loud. Report skipped tests, unverified cases, and partial results as such. Never present partial work as done.

## Definition of done
- All commands above pass. No `todo!()`, `unimplemented!()`, or `// TODO` in merged code.
- New behavior has tests. Modrinth, CurseForge, DNS, and Playit calls are mocked; no live network in tests.
- Spec criteria tagged `[auto]` are covered by tests. List `[manual]` ones in the PR for a human to verify.
- One feature or fix per PR. Conventional commit title: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.

## Code conventions
- `tokio` for all I/O. Never block async or UI threads (`spawn_blocking` if needed). No `std::thread::sleep`.
- Errors: `anyhow` in app code, `thiserror` in library modules. No `unwrap()`/`expect()` outside tests.
- Logging: `tracing`. Never log secrets.
- Public items get a doc comment. App config is human-editable TOML.
- One module per feature under `src/` (net, server, mods, instances, monitor, players, config). No god objects.
- Secrets (API keys, DNS tokens) live in the OS keychain via `keyring`, never in config or logs.
- Validate all external input: user input, API responses, file contents.

## Ask first (stop and request approval)
- Deleting files or directories; changing public API signatures
- Adding a dependency outside the approved list
- Changing config format, DB schema, or network protocol
- Anything that breaks backward compatibility

Approved crates: tokio, axum, serde, toml, anyhow, thiserror, tracing, rusqlite, reqwest, keyring, iroh, tauri, wiremock (dev).

## Hard constraints
- Never auto-update mods; the user confirms each update.
- Never write `eula.txt` until the user has seen and accepted the Mojang EULA. Download the server jar from Mojang at runtime; never bundle it.
- No telemetry, no mandatory account, only open formats (TOML, JSON, SQLite).
