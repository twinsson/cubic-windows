#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Prefer the newest built binary. Do not prefer ~/Downloads — that can be stale.
BIN_CANDIDATES=()
if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
  BIN_CANDIDATES+=("$CARGO_TARGET_DIR/release/minecraft-launcher")
fi
BIN_CANDIDATES+=(
  "$ROOT/src-tauri/target/release/minecraft-launcher"
  "$ROOT/target/release/minecraft-launcher"
  "$ROOT/dist-release/Cubic"
)

# Cursor/agent builds often land under /tmp/cursor-sandbox-cache/*/cargo-target
shopt -s nullglob
for c in /tmp/cursor-sandbox-cache/*/cargo-target/release/minecraft-launcher; do
  BIN_CANDIDATES+=("$c")
done
shopt -u nullglob

BIN=""
BIN_MTIME=0
for c in "${BIN_CANDIDATES[@]}"; do
  if [[ -x "$c" ]]; then
    mt=$(stat -c %Y "$c" 2>/dev/null || echo 0)
    if [[ -z "$BIN" || "$mt" -gt "$BIN_MTIME" ]]; then
      BIN="$c"
      BIN_MTIME=$mt
    fi
  fi
done

if [[ -z "$BIN" ]]; then
  echo "No Cubic binary found. Build first: pnpm tauri build --no-bundle"
  exit 1
fi

echo "Installing from: $BIN"

ICON="$ROOT/src-tauri/app-icon.png"
[[ -f "$ICON" ]] || ICON="$ROOT/src-tauri/icons/icon.png"

APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/cubic"
BIN_DIR="$HOME/.local/bin"
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"

mkdir -p "$APP_DIR" "$BIN_DIR" "$DESKTOP_DIR"
mkdir -p "$ICON_DIR"/{512x512,256x256,128x128,64x64,48x48,32x32}/apps
cp -f "$BIN" "$APP_DIR/Cubic"
chmod +x "$APP_DIR/Cubic"

cat > "$BIN_DIR/cubic" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/cubic"
if [[ -z "${GDK_BACKEND:-}" && -n "${WAYLAND_DISPLAY:-}" ]]; then
  export GDK_BACKEND=x11
fi
export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
exec "$APP_DIR/Cubic" "$@"
EOF
chmod +x "$BIN_DIR/cubic"
ln -sfn "$BIN_DIR/cubic" "$BIN_DIR/Cubic"

if command -v magick >/dev/null 2>&1; then
  magick "$ICON" -resize 512x512 "$ICON_DIR/512x512/apps/cubic.png"
  magick "$ICON" -resize 256x256 "$ICON_DIR/256x256/apps/cubic.png"
  magick "$ICON" -resize 128x128 "$ICON_DIR/128x128/apps/cubic.png"
  magick "$ICON" -resize 64x64 "$ICON_DIR/64x64/apps/cubic.png"
  magick "$ICON" -resize 48x48 "$ICON_DIR/48x48/apps/cubic.png"
  magick "$ICON" -resize 32x32 "$ICON_DIR/32x32/apps/cubic.png"
else
  cp -f "$ICON" "$ICON_DIR/128x128/apps/cubic.png"
fi

cat > "$DESKTOP_DIR/com.twinsson.cubic.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.5
Name=Cubic
GenericName=Minecraft Launcher
Comment=Minecraft Java Edition launcher
Exec=$BIN_DIR/cubic
Icon=cubic
Terminal=false
Categories=Game;AdventureGame;
Keywords=minecraft;launcher;java;mc;
StartupWMClass=Cubic
StartupNotify=true
EOF

update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
echo "Cubic installed. Search for 'Cubic' in your app launcher, or run: cubic"
