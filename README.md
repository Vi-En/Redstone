# Redstone

Redstone is an open-source, local-first Minecraft server host app built with Rust, Tauri 2, and Svelte. Everything runs on your own device — no cloud account, no telemetry, and no bundled proprietary binaries.

## Project status

All 13 milestones from [docs/specs/features.md](docs/specs/features.md) are implemented and covered by automated Rust and UI test suites:

- **Sticky Servers sidebar & per-instance isolation**: SQLite-backed CRUD, one-click switching, world-omitting duplication, and confirmation-gated deletion.
- **Runtime provisioning & compliance**: Official Mojang `server.jar` downloader at runtime, system Java version compatibility check, explicit user-confirmed Mojang EULA acceptance, and `crash-reports/` viewer.
- **Server lifecycle & Monitor tab**: Graceful stdin `stop` shutdown with timeout fallback, crash detection, real-time TPS (`<15` TPS alert), PID, uptime, CPU, RSS, JVM heap, RAM allocation, and live server console.
- **Interactive World & JVM configuration**: GUI controls for `server.properties` (preserving unknown/mod keys), `-Xmx` slider with 80% system memory warning, and Custom / Performance (Aikar) JVM presets.
- **Mod & modloader management**: Dual-catalog support for **Modrinth** and **CurseForge** Core APIs with token bucket rate limiting (120 req/min), in-memory TTL caching, exponential backoff retries, OS-keychain API key storage (`keyring`), distribution-disabled protection, safe primary JAR downloads, enable/disable toggles, and confirmation-gated mod updates and removals.
- **File explorer & Player management**: Sandboxed instance file browser/editor plus `whitelist.json` and `banned-players.json` management.
- **Networking, Dynamic DNS & Playit.gg**: LAN/public join card with pure-SVG QR codes, OS-keychain secret storage (`keyring`), and live sync for DuckDNS, deSEC, Cloudflare, and Playit.gg tunnels.
- **Remote control (Iroh) & Mobile Companion View**: Ed25519-verified Iroh pairing tickets, device revocation, and a touch-friendly (`≥44px` targets) Mobile Companion UI while keeping Minecraft player traffic on plain TCP.

## First-run setup & configuration

- Step-by-step setup guide: [docs/onboarding.md](docs/onboarding.md)
- Sample human-editable settings file: [config.example.toml](config.example.toml) (secrets stay in the OS keychain, never in TOML or logs)

## Development

All checks must pass before every commit:

- Rust format check: `cargo fmt --all -- --check`
- Rust lint: `cargo clippy --all-targets -- -D warnings`
- Rust tests: `cargo test --workspace`
- UI typecheck & tests: `npm --prefix ui run check && npm --prefix ui run test`
- Run desktop app: `cargo tauri dev`

### Preview and mobile companion testing

The UI can be previewed in a browser or from an Android emulator/device using `localStorage`-backed preview adapters (without invoking native Tauri IPC or launching Java):

```sh
npm --prefix ui run dev -- --host 0.0.0.0
```

- **Desktop host preview**: `http://127.0.0.1:5173/`
- **Mobile Companion view**: `http://127.0.0.1:5173/?mode=mobile` (or click **Mobile View** in the top bar)
- **Android Studio emulator**: `http://10.0.2.2:5173/`
- **Physical mobile device on LAN**: `http://<host-lan-ip>:5173/?mode=mobile` (or use `adb reverse tcp:5173 tcp:5173` with USB debugging)

Use `cargo tauri dev` to test native commands, filesystem sandboxing, OS keychain integration, Mojang runtime downloads, and Java subprocess management.

## Structure

- `src-tauri/` — Rust backend and Tauri IPC commands (`config`, `console`, `explorer`, `instances`, `jvm`, `mods`, `monitor`, `network`, `players`, `remote`, `server`, `worlds`)
- `ui/` — Svelte + TypeScript frontend and Vitest component/unit suites
- `docs/onboarding.md` — First-run setup guide
- `docs/specs/features.md` — Feature specifications and `[auto]` / `[manual]` acceptance criteria
- `config.example.toml` — Sample user settings file
- `.github/workflows/ci.yml` — CI workflow for Rust and UI checks

## License

This project is licensed under the [MIT License](LICENSE).
