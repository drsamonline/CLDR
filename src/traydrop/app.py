"""TrayDrop application entry point.

Behaviour (as launched by ``scripts/launch_traydrop.*`` / the desktop shortcut):

1. Acquire the single-instance lock.  If another copy is already running,
   bring *its* console window to the foreground and exit immediately - this
   is what makes re-clicking the shortcut "open the command window".
2. Otherwise: set the console title, hide the window, start a tray icon, and
   run the interactive console loop.
3. Tray menu **Show Console** restores + foregrounds the window;
   **Quit** stops the worker and exits (releasing the lock).
"""

from __future__ import annotations

import os
import signal
import sys
import threading
import time

from traydrop import __app_name__, __version__
from traydrop.single_instance import AlreadyRunning, SingleInstance
from traydrop.tray import TrayManager
from traydrop import window

CONSOLE_TITLE = f"{__app_name__} Console"
LOCK_NAME = "TrayDropSingleInstance"


class DemoWorker:
    """Placeholder for your real background workload.

    Replace :meth:`run` with whatever the console app should do.  The loop
    checks :attr:`stop_event` so **Quit** from the tray shuts it down cleanly.
    """

    def __init__(self) -> None:
        self.stop_event = threading.Event()
        self.started_at = time.time()
        self.iterations = 0

    def run(self) -> None:
        print(f"[{__app_name__}] worker started (pid {os.getpid()})")
        while not self.stop_event.is_set():
            self.iterations += 1
            print(f"[worker] tick #{self.iterations}")
            self.stop_event.wait(5.0)
        print("[worker] stopped.")


# --------------------------------------------------------------- console UI
def _console_loop(worker: DemoWorker, tray: TrayManager) -> None:
    """Simple REPL shown in the (hidden) command window."""
    banner = (
        f"\n=== {__app_name__} v{__version__} ===\n"
        "Window is hidden in the tray. Double-click the shortcut or use\n"
        "the tray menu to bring this window forward.\n"
        "Commands: show | hide | status | quit\n"
    )
    print(banner)
    while not worker.stop_event.is_set():
        try:
            line = input(f"{__app_name__}> ").strip().lower()
        except (EOFError, KeyboardInterrupt):
            break
        if line == "show":
            window.show_console()
        elif line == "hide":
            window.hide_console()
        elif line == "status":
            up = int(time.time() - worker.started_at)
            print(f"running {up}s, ticks={worker.iterations}")
        elif line in ("quit", "exit"):
            break
        elif line:
            print("unknown command")
    shutdown(worker, tray)


_SHUTTING_DOWN = False


def shutdown(worker: DemoWorker, tray: TrayManager) -> None:
    global _SHUTTING_DOWN
    if _SHUTTING_DOWN:
        return
    _SHUTTING_DOWN = True
    print("\nShutting down...")
    worker.stop_event.set()
    tray.stop()
    # Unhide before exit so any final messages are visible on Windows too.
    window.show_console()
    sys.exit(0)


# ---------------------------------------------------------------------- main
def main(argv: list[str] | None = None) -> int:
    args = argv if argv is not None else sys.argv[1:]
    verbose = "--verbose" in args or "-v" in args

    instance = SingleInstance(LOCK_NAME)
    try:
        instance.acquire()
    except AlreadyRunning:
        # Second launch via the shortcut: wake the existing window, then exit.
        instance.notify_running()          # POSIX: SIGUSR1 toggle
        if not window.show_console():      # Windows: FindWindow/SetForeground
            print("TrayDrop is already running - its window was raised.")
        return 0

    window.set_console_title(CONSOLE_TITLE)
    worker = DemoWorker()

    def on_show() -> None:
        window.show_console()

    def on_quit() -> None:
        shutdown(worker, tray)

    tray = TrayManager(on_show=on_show, on_quit=on_quit)
    started = tray.start()
    if not started and not verbose:
        print("warning: tray unavailable, running with visible console only")

    # Hide the console right away -> "runs in background".
    window.hide_console()
    if started:
        tray.notify("Running in the background. Click the icon to open the console.")

    # SIGUSR1 from a second launch toggles the window forward.
    if not window.IS_WINDOWS:
        signal.signal(signal.SIGUSR1, lambda *_: window.toggle_console())

    t = threading.Thread(target=worker.run, daemon=True)
    t.start()

    try:
        _console_loop(worker, tray)
    finally:
        worker.stop_event.set()
        tray.stop()
        instance.release()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
