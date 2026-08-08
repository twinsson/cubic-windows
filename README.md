# Cubic for Windows

Windows port of **Cubic**, the Minecraft Java Edition launcher. This is a **separate** project from the Linux Cubic app (`~/Projects/minecraft-launcher`) — changes here do not affect the Linux build.

## Requirements (on a Windows machine)

- [Node.js](https://nodejs.org/) 20+ and [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) stable
- [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (usually already installed on Windows 10/11)
- Visual Studio Build Tools with the “Desktop development with C++” workload (for Tauri)

## Develop

```powershell
cd cubic-windows
pnpm install
pnpm tauri dev
```

## Build installers

```powershell
cd cubic-windows
pnpm install
pnpm tauri build
```

Outputs (typical):

- `src-tauri\target\release\bundle\nsis\*.exe` — NSIS installer
- `src-tauri\target\release\bundle\msi\*.msi` — MSI installer
- `src-tauri\target\release\cubic.exe` — raw binary

Or run:

```powershell
.\scripts\build-windows.ps1
```

## Data locations

On Windows, Cubic stores data under:

- Config: `%APPDATA%\com\twinsson\Cubic\`
- Data (instances, libraries, assets, Java runtimes): `%LOCALAPPDATA%\com\twinsson\Cubic\` (via `directories` crate layout)

## Features carried over

- Offline username login (Microsoft login still available when Azure app ID is set)
- Automatic Mojang Java runtime download when the game needs a newer JDK than the system provides
- Fabric / Quilt / vanilla instances, Modrinth mods, skyblock home UI

## Note

Cross-compiling this Tauri app *from Linux to Windows* is possible but painful (needs a Windows WebView2 SDK / linker setup). Prefer building on Windows, or in a Windows CI runner / VM.
