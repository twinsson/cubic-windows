# Build Cubic Windows installers (run in PowerShell on Windows)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
  Write-Error "pnpm is required. Install Node.js, then: npm install -g pnpm"
}

pnpm install
pnpm tauri build

Write-Host ""
Write-Host "Done. Look under src-tauri\target\release\bundle\ for NSIS (.exe) and MSI installers."
