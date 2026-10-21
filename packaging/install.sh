#!/usr/bin/env bash
# =============================================================================
# CLDR — Command Line Dispatch & Route  (Linux installer)
# © 2026 Dr. Sohil Momin (drsamonline). MIT-licensed: retain this notice.
# =============================================================================
# Installs the cldr binary that sits next to this script:
#   ~/.local/bin/cldr                                  (on PATH)
#   ~/.local/share/applications/com.drsamonline.cldr.desktop
#   ~/.config/autostart/com.drsamonline.cldr.desktop   (tray at login)
#   a systemd --user service when systemd is present
#   an sxhkd Super+Alt+C summon binding when sxhkd is present
#
#   ./install.sh              # user install (default)
#   PREFIX=/usr/local sudo ./install.sh   # system-wide
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_SRC="$HERE/cldr"
[ -x "$BIN_SRC" ] || { echo "error: '$BIN_SRC' not found/executable" >&2; exit 1; }

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APP_DIR="$HOME/.local/share/applications"
AUTO_DIR="$HOME/.config/autostart"
EXE="$BIN_DIR/cldr"

mkdir -p "$BIN_DIR" "$APP_DIR" "$AUTO_DIR"
install -m 0755 "$BIN_SRC" "$EXE"
echo "[OK] $EXE"

cat > "$APP_DIR/com.drsamonline.cldr.desktop" <<EOF
# CLDR launcher entry — © 2026 Dr. Sohil Momin (drsamonline)
[Desktop Entry]
Type=Application
Name=CLDR — Command Line Dispatch & Route
Comment=Minimal tray-resident command launcher by Dr. Sohil Momin
Exec=$EXE
Icon=utilities-terminal
Terminal=true
Categories=Utility;System;
Keywords=launcher;palette;summon;cldr;
EOF
echo "[OK] application menu entry"

cat > "$AUTO_DIR/com.drsamonline.cldr.desktop" <<EOF
# CLDR tray autostart — © 2026 Dr. Sohil Momin (drsamonline)
[Desktop Entry]
Type=Application
Name=CLDR Tray
Exec=$EXE --tray
Terminal=false
X-GNOME-Autostart-enabled=true
EOF
echo "[OK] autostart (background tray) entry"

if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
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

if command -v sxhkd >/dev/null 2>&1; then
  mkdir -p "$HOME/.config/sxhkd/sxhkdrc.d"
  cat > "$HOME/.config/sxhkd/sxhkdrc.d/cldr.sxhkd" <<EOF
# CLDR summon hotkey — © 2026 Dr. Sohil Momin (drsamonline)
super + alt + c
    $EXE --summon
EOF
  echo "[OK] sxhkd binding: Super+Alt+C (reload sxhkd)"
fi

"$EXE" --config >/dev/null 2>&1 || true
echo
echo "Done. Press Super+Alt+C (or run 'cldr --summon') to bring the command window forward."
