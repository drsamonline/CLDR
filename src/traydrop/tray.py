"""Notification-area (system tray) integration built on *pystray*.

The tray icon is drawn programmatically with Pillow so the package ships
without binary assets.  Clicking / choosing **Show Console** calls back into
the platform-specific window helpers (:mod:`traydrop.window`);
**Quit** tears down the console app cleanly.
"""

from __future__ import annotations

import threading
from typing import Callable, Optional

from PIL import Image, ImageDraw

APP_NAME = "TrayDrop"


def make_icon_image(color: str = "#2d7ff9") -> Image.Image:
    """Return a simple 64x64 tray icon (rounded square + 'terminal' glyph)."""
    size = 64
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle([2, 2, size - 2, size - 2], radius=14, fill=color)
    # ">" prompt glyph
    draw.line([(16, 22), (30, 32), (16, 42)], fill="white", width=6, joint="curve")
    draw.line([(34, 44), (50, 44)], fill="white", width=6)
    return img


class TrayManager:
    """Owns the pystray icon and runs its event loop in a daemon thread."""

    def __init__(
        self,
        on_show: Callable[[], None],
        on_quit: Callable[[], None],
    ) -> None:
        self._on_show = on_show
        self._on_quit = on_quit
        self._icon = None  # type: ignore[assignment]
        self._thread: Optional[threading.Thread] = None

    # ------------------------------------------------------------ lifecycle
    def start(self) -> bool:
        """Start the tray loop. Returns ``False`` if no tray is available."""
        try:
            import pystray
            from pystray import Menu, MenuItem as Item

            self._icon = pystray.Icon(
                APP_NAME,
                icon=make_icon_image(),
                title=f"{APP_NAME} - running in background (click to show console)",
                menu=Menu(
                    Item("Show Console", self._show, default=True),
                    Item("Hide Console", self._hide),
                    pystray.Menu.SEPARATOR,
                    Item(f"About {APP_NAME}", self._about),
                    pystray.Menu.SEPARATOR,
                    Item("Quit", self._quit),
                ),
            )
            self._thread = threading.Thread(target=self._icon.run, daemon=True)
            self._thread.start()
            return True
        except Exception:  # pragma: no cover - headless / missing backend
            # ImportError (dependency missing) or a display-backend failure
            # (no X11/Win32 tray available). Caller degrades gracefully.
            self._icon = None
            return False

    def stop(self) -> None:
        if self._icon is not None:
            try:
                self._icon.stop()
            finally:
                self._icon = None

    def notify(self, message: str, title: str = APP_NAME) -> None:
        """Best-effort balloon notification (ignored when no icon is active)."""
        if self._icon is not None:
            try:
                self._icon.notify(message, title)
            except Exception:  # pragma: no cover - desktop-dependent
                pass

    # -------------------------------------------------------------- actions
    def _show(self, *_args) -> None:
        self._on_show()

    def _hide(self, *_args) -> None:
        from traydrop import window

        window.hide_console()

    def _about(self, *_args) -> None:
        self.notify("Background console helper v1.0.0\nDocs: see README.md")

    def _quit(self, *_args) -> None:
        self._on_quit()
