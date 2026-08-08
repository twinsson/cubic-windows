# Cubic for Windows

Minecraft Java Edition launcher for Windows.

## For players (install Cubic)

You do **not** need Git, Node, or Rust.

1. Download the latest **`.exe` installer** from  
   [Releases](https://github.com/twinsson/cubic-windows/releases)  
   (or grab the artifact from the newest successful **Actions** run).
2. Run the installer.
3. Open **Cubic** from the Start menu.

If there is no Release yet, ask whoever maintains Cubic to publish one — building from source is only for developers.

## For developers (build from source)

You need:

- [Git](https://git-scm.com/download/win)
- [Node.js 20+](https://nodejs.org/) then `npm install -g pnpm`
- [Rust](https://rustup.rs/)
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**
- WebView2 (usually already on Windows 10/11)

```powershell
git clone https://github.com/twinsson/cubic-windows.git
cd cubic-windows
pnpm install
pnpm tauri build
```

Installers appear under `src-tauri\target\release\bundle\nsis\` and `msi\`.

Or run:

```powershell
.\scripts\build-windows.ps1
```

## Data locations

- Config: `%APPDATA%\com\twinsson\Cubic\`
- Game data / Java runtimes: under the Cubic data dir from the `directories` crate (`%LOCALAPPDATA%\com\twinsson\Cubic\` layout)
