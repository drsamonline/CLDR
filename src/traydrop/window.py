"""Platform window helpers: hide / show / foreground the console window.

Windows uses Win32 via :mod:`ctypes`.  Linux/macOS fall back to best-effort
approaches (``xdotool`` when present) and are documented as such in the
README - the *primary* target for the hidden-console workflow is Windows,
where ``pythonw.exe``/``VBS`` launchers make it seamless.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys

IS_WINDOWS = sys.platform == "win32"
IS_MACOS = sys.platform == "darwin"


def set_console_title(title: str) -> None:
    """Tag the console so the second-launch shortcut can find it by title."""
    if IS_WINDOWS:
        os.system(f"title {title}")  # noqa: S605 - fixed literal in cmd context
    else:
        # ANSI escape understood by most terminal emulators.
        sys.stdout.write(f"\033]0;{title}\a")
        sys.stdout.flush()
    os.environ["TRAYDROP_TITLE"] = title


def _sw_hide() -> int:
    return 0  # SW_HIDE


def hide_console() -> bool:
    """Hide the console window (app keeps running)."""
    if IS_WINDOWS:
        import ctypes

        hwnd = ctypes.windll.kernel32.GetConsoleWindow()  # type: ignore[attr-defined]
        if hwnd:
            ctypes.windll.user32.ShowWindow(hwnd, _sw_hide())  # type: ignore[attr-defined]
            return True
        return False
    if not IS_MACOS and shutil.which("xdotool"):
        wid = _xterm_window_id()
        if wid:
            subprocess.run(["xdotool", "windowminimize", wid], check=False)
            return True
    return False


def show_console() -> bool:
    """Restore and bring the console window to the foreground."""
    if IS_WINDOWS:
        import ctypes

        user32 = ctypes.windll.user32  # type: ignore[attr-defined]
        kernel32 = ctypes.windll.kernel32  # type: ignore[attr-defined]
        hwnd = kernel32.GetConsoleWindow()
        if not hwnd:
            return False
        SW_SHOWMINIMIZED, SW_RESTORE, SW_SHOW = 2, 9, 5
        if user32.IsIconic(hwnd):
            user32.ShowWindow(hwnd, SW_RESTORE)
        else:
            user32.ShowWindow(hwnd, SW_SHOW)
        user32.SetForegroundWindow(hwnd)
        return True
    if not IS_MACOS and shutil.which("xdotool"):
        wid = _xterm_window_id()
        if wid:
            subprocess.run(["xdotool", "windowmap", "--sync", wid], check=False)
            subprocess.run(["xdotool", "windowactivate", "--sync", wid], check=False)
            subprocess.run(["xdotool", "windowraise", wid], check=False)
            return True
    return False


def toggle_console() -> bool:
    """Show the window if hidden/minimised, otherwise hide it."""
    return show_console()


def _xterm_window_id() -> str | None:
    pid = str(os.getpid())
    try:
        out = subprocess.run(
            ["xdotool", "search", "--pid", pid],
            capture_output=True, text=True, timeout=2, check=False,
        ).stdout.strip().splitlines()
        return out[0] if out else None
    except (subprocess.SubprocessError, OSError):  # pragma: no cover
        return None
