# Cubic — Minecraft Java Launcher

Linux-first Minecraft: Java Edition launcher built with **Rust** and **Tauri 2**.

Cubic never asks for your Microsoft password. Sign-in opens the system browser (OAuth 2.0 device code). Refresh tokens live in the **system keyring**. Game files are stored under **XDG** directories and verified with Mojang SHA1 hashes.

## Features (v1)

- Microsoft account sign-in via system browser
- Vanilla instance create / install / launch
- Shared libraries, assets, and versions cache
- Cancellable downloads with progress events
- Hash verification for every hashed artifact
- Java detection (`JAVA_HOME`, `PATH`, `/usr/lib/jvm`)
- Ownership check through Minecraft entitlements (no bypasses)

## Requirements

- Linux (primary)
- Rust toolchain (edition 2021+)
- Node.js + pnpm
- Tauri 2 Linux prerequisites ([guide](https://tauri.app/start/prerequisites/))
  - On Arch/CachyOS: `sudo pacman -S webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg`
- A Java runtime matching the instance’s required major version (e.g. 17 / 21)
- `unzip` (used to extract native libraries)

## Microsoft sign-in (shows as **Cubic**, not Prism)

Microsoft labels the consent screen with the **Azure app name** tied to the client ID.
Cubic no longer uses Prism’s ID.

### One-time setup

1. In Cubic click **Create Cubic app** (or open [Azure app registration](https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/CreateApplicationBlade)).
2. Name the app exactly **`Cubic`**.
3. Account type: personal Microsoft accounts (and optionally any org).
4. After create → **Authentication** → enable **Allow public client flows**.
5. Copy **Application (client) ID** into Cubic → Settings → **Cubic Microsoft app ID** → Save.
6. Submit that client ID for Minecraft API access: [aka.ms/mce-reviewappid](https://aka.ms/mce-reviewappid) (also **Mojang approve ID** in Cubic). Approval can take time.
7. Sign out (if you were signed in under Prism) and **Sign in with Microsoft** again.

Passwords are never typed into Cubic. The refresh token is stored only in the system keyring.

## XDG layout

| Path | Purpose |
|------|---------|
| `~/.config/minecraft-launcher/settings.json` | Memory, selected instance |
| `~/.local/share/minecraft-launcher/instances/<id>/` | `instance.json` + `minecraft/` game dir |
| `~/.local/share/minecraft-launcher/libraries/` | Shared libraries |
| `~/.local/share/minecraft-launcher/assets/` | Shared assets |
| `~/.local/share/minecraft-launcher/versions/` | Version JSON + client jars |
| `~/.cache/minecraft-launcher/downloads/` | Temporary download parts |
| System keyring service `com.twinsson.cubic` | Microsoft refresh token only |

## Develop

```bash
pnpm install
pnpm tauri dev
```

Release build:

```bash
pnpm tauri build
```

## Architecture

Rust modules under `src-tauri/src/`:

- `auth/` — browser OAuth, MSA → Xbox → XSTS → Minecraft services, keyring
- `metadata/` — piston-meta version manifest / version JSON / asset index
- `download/` — concurrent fetch, SHA1 verify, cancel tokens, progress events
- `java/` — JVM discovery and major-version matching
- `launch/` — classpath, argument substitution, process spawn
- `instance/` — vanilla instance records on disk
- `paths.rs` / `settings.rs` / `error.rs` — XDG paths, config, typed errors

Production Rust code avoids `unwrap()` / `expect()`.

## Out of scope (v1)

Fabric / Forge / Quilt, automatic JRE download, mod browsers, offline “cracked” multiplayer identities.
