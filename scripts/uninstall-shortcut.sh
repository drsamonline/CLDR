#!/usr/bin/env bash
# CLDR uninstaller — © 2026 Dr. Sohil Momin (drsamonline). Retain this notice.
set -euo pipefail
"$HOME/.local/bin/cldr" --daemon-stop 2>/dev/null || true
rm -f "$HOME/.local/bin/cldr"       "$HOME/.local/share/applications/com.drsamonline.cldr.desktop"       "$HOME/.config/autostart/com.drsamonline.cldr.desktop"       "$HOME/.config/sxhkd/sxhkdrc.d/cldr.sxhkd"
systemctl --user disable --now cldr-tray.service 2>/dev/null || true
rm -f "$HOME/.config/systemd/user/cldr-tray.service"
systemctl --user daemon-reload 2>/dev/null || true
echo "[OK] CLDR shortcuts removed. Your dispatch state dir (~/$${XDG_RUNTIME_DIR:-/tmp}/cldr-daemon) may remain."
