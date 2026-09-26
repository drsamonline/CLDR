#!/usr/bin/env bash
# =============================================================================
# CLDR — Command Line Dispatch & Route
# © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
# MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
# ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
# watermark is a violation of the license terms.
# =============================================================================
# Launch-shortcut installer (Linux / XDG). Creates:
#   ~/.local/bin/cldr                              CLI on PATH
#   ~/.local/share/applications/com.drsamonline.cldr.desktop   app-launcher entry
#   ~/.config/autostart/com.drsamonline.cldr.desktop           tray-resident autostart
#   ~/.config/sxhkd/sxhkdrc.d/cldr.sxhkd                       Super+Alt+C summon hotkey (if sxhkd present)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXE="$ROOT/target/release/cldr"
[ -x "$EXE" ] || EXE="$ROOT/target/debug/cldr"
[ -x "$EXE" ] || { echo "error: build first (cargo build --release)" >&2; exit 1; }

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"          "$HOME/.config/autostart" "$HOME/.config/cldr"

ln -sf "$EXE" "$HOME/.local/bin/cldr"
echo "[OK] $HOME/.local/bin/cldr -> $EXE"

cat > "$HOME/.local/share/applications/com.drsamonline.cldr.desktop" <<EOF
# CLDR launcher entry — © 2026 Dr. Sohil Momin (drsamonline)
[Desktop Entry]
Type=Application
Name=CLDR — Command Line Dispatch & Route
Comment=Minimal command launcher by Dr. Sohil Momin
Exec=$EXE
Icon=utilities-terminal
Terminal=true
Categories=Utility;System;
Keywords=launcher;palette;summon;cldr;
EOF
echo "[OK] application menu entry"

cat > "$HOME/.config/autostart/com.drsamonline.cldr.desktop" <<EOF
# CLDR tray autostart — © 2026 Dr. Sohil Momin (drsamonline)
[Desktop Entry]
Type=Application
Name=CLDR Tray
Exec=$EXE --tray
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
echo "[OK] autostart (background tray) entry"

if command -v sxhkd >/dev/null 2>&1; then
  mkdir -p "$HOME/.config/sxhkd/sxhkdrc.d"
  cat > "$HOME/.config/sxhkd/sxhkdrc.d/cldr.sxhkd" <<EOF
# CLDR summon hotkey — © 2026 Dr. Sohil Momin (drsamonline)
super + alt + c
    $EXE --summon
EOF
  echo "[OK] sxhkd binding: Super+Alt+C -> cldr --summon (reload sxhkd)"
else
  echo "[i] sxhkd not installed — see docs/SHORTCUTS.md for KDE/GNOME binding steps"
fi

# Keep the daemon alive under systemd user sessions when available
if command -v systemctl >/dev/null 2>&1 && [ -d "$HOME/.config/systemd/user" -o ! -e /run/systemd/generator ]; then
  mkdir -p "$HOME/.config/systemd/user"
  cat > "$HOME/.config/systemd/user/cldr-tray.service" <<EOF
# CLDR background service unit — © 2026 Dr. Sohil Momin (drsamonline)
[Unit]
Description=CLDR tray-resident background launcher
After=graphical-session.target

[Service]
ExecStart=$EXE --tray
Restart=on-failure
Nice=10

[Install]
WantedBy=default.target
EOF
  systemctl --user daemon-reload 2>/dev/null || true
  systemctl --user enable cldr-tray.service 2>/dev/null || true
  echo "[OK] systemd user service 'cldr-tray' enabled (best effort)"
fi

echo
echo "Done. Press Super+Alt+C (or run 'cldr --summon') to bring the command window forward."
