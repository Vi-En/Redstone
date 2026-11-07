# Feature specs

This project follows the 13-step build order and acceptance criteria below. Criteria tagged `[auto]` are verified by automated Rust (`cargo test --workspace`) and UI (`npm --prefix ui run test`) suites; criteria tagged `[manual]` require human verification in `cargo tauri dev` with live external services or a Minecraft client.

## Build order
1. Project skeleton, `Cargo.toml`, CI
2. Minecraft server manager (subprocess, lifecycle, config files)
3. Instances (CRUD, isolation, SQLite)
4. Mod and modloader management
5. Live console and logs
6. Worlds, RAM, JVM args
7. Resource monitor
8. File explorer
9. Player management
10. Networking: public address, DNS, tunnel
11. Remote control (Iroh) and mobile view
12. Settings, onboarding — see [docs/onboarding.md](../onboarding.md)
13. README and `config.example.toml`

## Settings and onboarding
- [auto] A missing settings file loads `onboarding_complete = false`, `lan_only = true`, and no instance.
- [auto] Selecting an instance or saving server settings sets `onboarding_complete = true`.
- [auto] Native startup refuses to launch until `server.jar` exists and `eula.txt` contains `eula=true`. The app never writes `eula.txt` without explicit user confirmation.
- [manual] A new desktop user can create an instance, provision `server.jar`, explicitly confirm the Mojang EULA, leave the server on LAN, and start the server.

## Instances and sidebar management
- [auto] Instances persist in SQLite and isolate directories per instance (`mods/`, `world/`, `server.properties`, `jvm.json`).
- [auto] Duplicating an instance copies files and metadata (`minecraft_version`, `loader`) while omitting `world/`.
- [auto] Deleting an instance requires explicit confirmation and refuses to delete a running server.
- [auto] Sticky left Servers sidebar lists instances, switches active context, duplicates, and deletes instances.

## Server management and runtime provisioning
- [auto] Official `server.jar` is downloaded at runtime from Mojang's version manifest (`wiremock`-tested), never bundled, and refuses to overwrite an existing `server.jar`.
- [auto] Java major version output (`java -version`) is parsed and compared against the Minecraft version requirement.
- [auto] Accepting the Mojang EULA requires explicit user confirmation (`confirmed = true` / checked EULA agreement box) before writing `eula=true`.
- [auto] Latest crash report under `crash-reports/` is read and surfaced in the Server Management provisioning card.
- [auto] Native lifecycle commands reject a missing `server.jar` or unaccepted EULA, send graceful `stop` over stdin before killing on timeout, and emit state transitions.
- [auto] Preview lifecycle controls persist running/stopped state without launching Java.
- [auto] Console commands reject empty or multi-line input.
- [auto] Native stdout and stderr lines are parsed into per-instance console events preserving INFO, WARN, and ERROR levels.
- [manual] A configured Java server downloads `server.jar` from live Mojang CDN, starts cleanly, and accepts a vanilla Minecraft client connection.

## World and server properties
- [auto] The complete `server.properties` map round-trips through the interactive GUI editor without losing unknown or mod-added fields.
- [auto] Preview properties persist per instance without writing native files.
- [auto] Native property reads and writes are scoped to the selected instance.
- [manual] A Minecraft server uses the saved world properties on its next start.

## RAM and JVM arguments
- [auto] JVM settings persist per instance and are applied to the next native server start.
- [auto] The Performance preset emits Aikar-style JVM flags.
- [auto] Custom JVM arguments remain line-oriented and are passed verbatim.
- [auto] The UI warns when RAM allocation exceeds 80% of detected system memory.
- [auto] Switching from dirty custom arguments to a preset requires confirmation.

## Resource monitor and live console
- [auto] Preview and native resource metrics are read through a non-blocking adapter (`spawn_blocking`).
- [auto] Dedicated Monitor tab displays TPS, PID, uptime, CPU, RSS memory, JVM heap, RAM allocation, and the live server console.
- [auto] Dedicated Monitor cards render functioning real-time SVG sparkline/area graphs for tick rate (TPS), CPU usage, process RAM, and JVM heap with min/peak/avg readouts and live markers.
- [auto] Low-TPS alert state is surfaced when the monitor model reports sustained TPS below 15 for over 10 seconds.
- [manual] Native CPU, memory, heap, PID, uptime, and TPS values match an external system check while a Java server runs.

## Mod catalog and installed mods
- [auto] Modrinth search results are parsed from mocked API responses with Minecraft version and loader filters.
- [auto] CurseForge search results are parsed from mocked API responses with Minecraft version and loader filters using user-supplied API key.
- [auto] CurseForge API enforces token bucket rate limiting (120 req/min) and retries transient failures with exponential backoff (max 3 attempts).
- [auto] CurseForge search results are cached in-memory with a 5-minute TTL, making no network calls on repeat requests within the window.
- [auto] If a CurseForge mod has distribution disabled (`allowModDistribution = false` or missing file URL), download is refused and project page link is returned.
- [auto] Wiremock tests verify CurseForge rate-limited responses, malformed JSON schemas, distribution-disabled guards, and version conflicts.
- [auto] CurseForge API keys are stored in the OS keychain via `keyring` / `SecretStore`, never in config or logs.
- [auto] UI search waits 250 ms after the latest input before invoking the catalog adapter.
- [auto] UI catalog source selector switches between authentic Modrinth and CurseForge themed catalogs with inline keychain key management, functioning category filter chips, environment tags, and distribution-disabled link presentation.
- [auto] Browser preview returns local sample results for both Modrinth and CurseForge without invoking native commands.
- [auto] Installing a selected result downloads the primary JAR without overwriting existing files.
- [auto] Installed JARs can be listed and enabled or disabled (`.jar.disabled`) within their instance.
- [auto] Removing an installed JAR requires explicit confirmation and cannot escape the instance `mods/` directory.
- [auto] Update badge counts newer compatible versions for tracked Modrinth and CurseForge installs, and applying an update requires explicit confirmation.
- [manual] The desktop app searches and installs mods from the live Modrinth and CurseForge Core APIs and loads them in-game.

## File explorer
- [auto] Directory listing, file reads, file writes, and confirmation-gated deletions reject `..` traversal and paths escaping the instance root.
- [auto] Files tab navigates subdirectories, edits text files, and deletes entries after confirmation.
- [manual] Editing a configuration file in the Files tab updates the file on disk inside the selected instance folder.

## Player management
- [auto] `whitelist.json` and `banned-players.json` round-trip cleanly per instance with deterministic offline UUID generation in preview/offline workflows.
- [auto] Players tab adds and removes whitelisted and banned players.
- [manual] Whitelist and banlist changes take effect on a running or restarted Minecraft server.

## Networking, Dynamic DNS, and Playit.gg
- [auto] LAN and public join addresses (Manual, DuckDNS, deSEC, Cloudflare, and Playit.gg) compute deterministic host:port endpoints and pure-SVG QR codes.
- [auto] DuckDNS, deSEC, Cloudflare, and Playit.gg HTTP clients are verified against mocked `wiremock` endpoints with no live network calls in tests.
- [auto] DNS and tunnel tokens are stored through the OS keychain (`keyring`) adapter, never in `settings.toml` or logs.
- [manual] Syncing DuckDNS, deSEC, Cloudflare, or Playit.gg updates a live DNS record or tunnel endpoint using a real token stored in the OS keychain.

## Remote control (Iroh) and Mobile Companion View
- [auto] Remote control pairing generates one-time tickets (`redstone://pair/<endpoint>/<ticket>`), verifies Ed25519 signatures, authorizes paired clients, and supports revocation.
- [auto] Mobile Companion View (`?mode=mobile` or topbar toggle) renders 2-level bottom navigation (**Server**, **Console**, **Players**) with `≥44px` touch targets while keeping Minecraft client traffic on plain TCP.
- [manual] Pairing a mobile browser or companion device over Iroh controls server start/stop, console commands, and whitelist entries remotely.
