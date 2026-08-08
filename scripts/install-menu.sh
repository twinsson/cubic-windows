#!/usr/bin/env bash
# Install Cubic into the system app menu (Noctalia / Hyprland), like Prism Launcher.
set -euo pipefail

USER_APP="${XDG_DATA_HOME:-$HOME/.local/share}/cubic/Cubic"
USER_ICON="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/128x128/apps/cubic.png"
USER_DESKTOP="${XDG_DATA_HOME:-$HOME/.local/share}/applications/com.twinsson.cubic.desktop"

if [[ ! -x "$USER_APP" ]]; then
  echo "Missing $USER_APP — put the Cubic binary there first."
  exit 1
fi

# Ensure user wrapper exists
mkdir -p "$HOME/.local/bin"
cat > "$HOME/.local/bin/cubic" <<'WRAP'
#!/usr/bin/env bash
set -euo pipefail
APP="${XDG_DATA_HOME:-$HOME/.local/share}/cubic/Cubic"
if [[ -z "${GDK_BACKEND:-}" && -n "${WAYLAND_DISPLAY:-}" ]]; then
  export GDK_BACKEND=x11
fi
export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
exec "$APP" "$@"
WRAP
chmod +x "$HOME/.local/bin/cubic"

# User desktop entry
mkdir -p "$(dirname "$USER_DESKTOP")"
cat > "$USER_DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Version=1.5
Name=Cubic
GenericName=Minecraft Launcher
Comment=Minecraft Java Edition launcher
Exec=$HOME/.local/bin/cubic
TryExec=$HOME/.local/bin/cubic
Icon=cubic
Terminal=false
Categories=Game;ActionGame;AdventureGame;
Keywords=minecraft;launcher;java;mc;cubic;
StartupWMClass=cubic
StartupNotify=true
EOF
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

# System-wide install (this is what makes Noctalia find it reliably, like Prism)
sudo mkdir -p /usr/local/bin /usr/local/share/applications \
  /usr/local/share/icons/hicolor/128x128/apps \
  /usr/local/share/icons/hicolor/256x256/apps \
  /usr/local/share/icons/hicolor/512x512/apps

sudo tee /usr/local/bin/cubic >/dev/null <<'EOF2'
#!/usr/bin/env bash
set -euo pipefail
APP="${XDG_DATA_HOME:-$HOME/.local/share}/cubic/Cubic"
if [[ ! -x "$APP" ]]; then
  echo "Cubic binary missing at $APP" >&2
  exit 1
fi
if [[ -z "${GDK_BACKEND:-}" && -n "${WAYLAND_DISPLAY:-}" ]]; then
  export GDK_BACKEND=x11
fi
export WEBKIT_DISABLE_COMPOSITING_MODE="${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
exec "$APP" "$@"
EOF2
sudo chmod +x /usr/local/bin/cubic

sudo tee /usr/local/share/applications/com.twinsson.cubic.desktop >/dev/null <<'EOF2'
[Desktop Entry]
Type=Application
Version=1.5
Name=Cubic
GenericName=Minecraft Launcher
Comment=Minecraft Java Edition launcher
Exec=/usr/local/bin/cubic
TryExec=/usr/local/bin/cubic
Icon=cubic
Terminal=false
Categories=Game;ActionGame;AdventureGame;
Keywords=minecraft;launcher;java;mc;cubic;
StartupWMClass=cubic
StartupNotify=true
EOF2

if [[ -f "$USER_ICON" ]]; then
  sudo cp "$USER_ICON" /usr/local/share/icons/hicolor/128x128/apps/cubic.png
fi
for sz in 256 512; do
  src="$HOME/.local/share/icons/hicolor/${sz}x${sz}/apps/cubic.png"
  if [[ -f "$src" ]]; then
    sudo cp "$src" "/usr/local/share/icons/hicolor/${sz}x${sz}/apps/cubic.png"
  fi
done

sudo update-desktop-database /usr/local/share/applications
sudo gtk-update-icon-cache -f -t /usr/local/share/icons/hicolor 2>/dev/null || true

echo
echo "Cubic is installed in the app menu."
echo "Open Noctalia launcher (Super+Space) and search: Cubic"
echo "If it still doesn't show, restart Noctalia or Hyprland once."
echo
echo "You can also run:  cubic"
