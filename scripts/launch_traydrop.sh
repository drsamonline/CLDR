#!/usr/bin/env bash
# ------------------------------------------------------------------
#  TrayDrop launcher (Linux / macOS)
#  * First run        -> starts the app (worker in background thread).
#  * Already running  -> SIGUSR1 is sent to the instance, raising or
#                        minimising its terminal window (needs xdotool
#                        on X11; best-effort elsewhere).
#  Make it executable:  chmod +x launch_traydrop.sh
#  Bind to a desktop shortcut: see docs/SETUP.md (Linux section).
# ------------------------------------------------------------------
set -euo pipefail
cd "$(dirname "$0")/.."

if command -v python3 >/dev/null 2>&1; then PY=python3; else PY=python; fi

# Prefer launching inside a real terminal so the "console window" exists.
case "${TERM_LAUNCHER:-auto}" in
  gnome-terminal) exec gnome-terminal --title="TrayDrop Console" -- "$PY" -m traydrop ;;
  konsole)        exec konsole -p "TitleMatchClass=TrayDrop" -- "$PY" -m traydrop ;;
  xterm)          exec xterm -T "TrayDrop Console" -e "$PY" -m traydrop ;;
  *)              exec "$PY" -m traydrop ;;
esac
