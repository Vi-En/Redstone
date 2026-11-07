# First-run setup

Redstone opens straight into the local control room with a sticky **Servers** sidebar on the left and six workspace tabs in the center (**Mods catalog**, **Installed mods**, **Server management**, **Monitor**, **Files**, and **Players**). Setup is finished when this device has saved settings and an instance is selected.

Settings are stored in `settings.toml` inside the app config directory. A missing file uses the defaults in `config.example.toml`. Tokens, API keys, and tunnel credentials stay in the OS keychain (`keyring`), never in `settings.toml` or logs.

## Defaults

| Setting | First-run value | What it means |
|---|---|---|
| `onboarding_complete` | `false` | This device has not finished setup. |
| `lan_only` | `true` | New servers stay on the local network. |
| `instance_name` | unset | No instance is selected. |
| `dns_provider` | unset | Used only when public access is turned on (`manual`, `duckdns`, `desec`, `cloudflare`, or `playit`). |
| `dns_subdomain` | unset | Single hostname label, such as `play`. |
| `dns_domain` | unset | Domain appended to the subdomain, such as `example.com`. |

## 1. Create, select, duplicate, or delete a server

In the left **Servers** sidebar, click **+** (Add Server), enter a server name (1–48 characters), a Minecraft version, and a loader (`vanilla`, `fabric`, `forge`, or `neoforge`), then click **Create Server**.

- Selecting a server card in the sidebar sets `onboarding_complete` to `true` and saves `settings.toml`.
- When a server is selected, **Duplicate** clones its configuration and mods into a new isolated directory without copying `world/`.
- **Delete** asks for explicit confirmation and refuses to delete a currently running server.

In the browser preview, this stores metadata in `localStorage`. The desktop app creates the isolated instance directory and SQLite record.

## 2. Choose who can join & share the address

Open **Server management**.

- **LAN only** is the default. Public DNS fields stay hidden, and **Server Join Address** shows the detected local IP and port with copy-to-clipboard and an expandable QR code.
- **Online** enables **Dynamic DNS Configuration** supporting:
  - **Manual DNS** (`{dns_subdomain}.{dns_domain}`)
  - **DuckDNS** (`{dns_subdomain}.duckdns.org`)
  - **deSEC** (`{dns_subdomain}.{dns_domain}`)
  - **Cloudflare** (`{dns_subdomain}.{dns_domain}` with Zone ID)
  - **Playit.gg** (`{dns_subdomain}.gl.joinmc.link` tunnel)

API and tunnel tokens are saved directly to the OS keychain via **Save to Keychain**. **Save server settings** validates hostname labels and sets `onboarding_complete` to `true`.

## 3. Provision `server.jar` and accept the Mojang EULA

Redstone never bundles Minecraft binaries and never writes `eula.txt` automatically. Under **Mojang Server Runtime & EULA** on **Server management**:

1. **Java check**: Redstone compares the detected system Java major version against the version required by Minecraft (for example, Java 21+ for Minecraft 1.20.5+ / 1.21.x) and links to Adoptium Temurin if an upgrade is needed.
2. **Download `server.jar`**: Click **Download server.jar** to fetch the official server binary directly from Mojang's version manifest at runtime without overwriting an existing JAR.
3. **Accept EULA**: Read the [official Mojang EULA](https://www.minecraft.net/eula), check **"I have read and agree to the Mojang EULA"**, and click **Accept EULA** to write `eula=true` to `eula.txt`.

Starting the server refuses to launch Java until both `server.jar` and `eula=true` exist in the instance directory. If a server crashes and writes a file under `crash-reports/`, the latest report is surfaced directly in this card.

## 4. Configure world properties, RAM, mods, files, and players

- **World and server properties** (**Server management**): Interactive controls for MOTD, world name/seed, gamemode, difficulty, gameplay/security toggles, player/chunk sliders, and collapsible custom/mod properties.
- **RAM and JVM arguments** (**Server management**): `-Xmx` slider (warns above 80% of system RAM), Custom vs Performance (Aikar-style) presets, and line-oriented custom flags.
- **Mods catalog & Installed mods**: Dual-source mod search supporting both **Modrinth** and **CurseForge** Core APIs. Filtered by selected Minecraft version and modloader. CurseForge API keys are stored securely in the OS keychain (`keyring`). Mods with disabled third-party distribution show direct external links to their project page instead of downloading. Installed mods support enable/disable (`.jar.disabled`), safe primary JAR updates with confirmation, and confirmation-gated removals.
- **Monitor**: Live process status, Start/Stop/Restart controls, Refresh status, TPS (with `<15` TPS alert), PID, uptime, CPU, RSS memory, JVM heap, RAM allocation, and live server console with command input.
- **Files & Players**: Instance-scoped file browser/editor and `whitelist.json` / `banned-players.json` managers.
- **Mobile Remote Control (Iroh)**: Generate a one-time QR/code pairing ticket on **Server management** to authorize a mobile companion over end-to-end encrypted Iroh without routing vanilla Minecraft TCP traffic through Iroh.

## What “setup complete” means

`onboarding_complete` becomes `true` only when you select an instance or save server settings. Opening the app, creating an instance without selecting it, or installing a mod does not flip the flag on its own.
