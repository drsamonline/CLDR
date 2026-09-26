"""Single-instance lock + "bring window forward" signalling.

How it works
------------
* Windows: a named OS mutex guarantees only one instance runs.  A second
  launch detects the mutex, calls ``SetForegroundWindow`` on the running
  instance's console and exits.
* Linux / macOS: an ``flock``-protected lock file stores the PID.  A second
  launch sends ``SIGUSR1`` to the running process, which toggles the
  terminal window visibility (hide/show) via the bundled shell helpers.
"""

from __future__ import annotations

import errno
import fcntl
import os
import signal
import sys
import tempfile
from pathlib import Path

IS_WINDOWS = sys.platform == "win32"


class AlreadyRunning(Exception):
    """Raised in the *second* process when another instance owns the lock."""


class SingleInstance:
    """Cross-platform single-instance guard.

    Usage::

        inst = SingleInstance("myapp")
        try:
            inst.acquire()          # raises AlreadyRunning if another copy is up
        except AlreadyRunning:
            sys.exit(0)             # the other copy was already woken up
    """

    def __init__(self, name: str) -> None:
        self.name = name
        self._lock_fd: int | None = None
        self._mutex_handle = None  # Windows only

    # ------------------------------------------------------------------ API
    def acquire(self) -> None:
        """Take ownership of the instance lock or raise :class:`AlreadyRunning`."""
        if IS_WINDOWS:
            self._acquire_windows()
        else:
            self._acquire_posix()

    def notify_running(self) -> None:
        """Ask the already-running instance to bring its window forward."""
        if not IS_WINDOWS:
            pid = self._read_pid()
            if pid:
                try:
                    os.kill(pid, signal.SIGUSR1)
                except OSError as exc:  # pragma: no cover - defensive
                    if exc.errno != errno.ESRCH:
                        raise

    @staticmethod
    def current_console_hwnd() -> int:
        """Return the native window handle of the attached console (Windows)."""
        import ctypes

        return ctypes.windll.kernel32.GetConsoleWindow()  # type: ignore[attr-defined]

    # -------------------------------------------------------------- Windows
    def _acquire_windows(self) -> None:
        import ctypes

        kernel32 = ctypes.windll.kernel32
        ERROR_ALREADY_EXISTS = 183

        handle = kernel32.CreateMutexW(None, False, f"Local\\{self.name}")
        if kernel32.GetLastError() == ERROR_ALREADY_EXISTS:
            # Another instance owns the mutex -> wake its console window.
            hwnd = kernel32.FindWindowW(None, _window_title())
            if hwnd:
                SW_RESTORE = 9
                kernel32.ShowWindow(hwnd, SW_RESTORE)
                kernel32.SetForegroundWindow(hwnd)
            else:
                # Fallback: signal via the mutex-holding process' console.
                hwnd = self.current_console_hwnd()
                if hwnd:
                    kernel32.AllowSetForegroundWindow(-1)  # ASFW_ANY
            self._close_handle(handle)
            raise AlreadyRunning

        self._mutex_handle = handle

    @staticmethod
    def _close_handle(handle) -> None:
        import ctypes

        try:
            ctypes.windll.kernel32.CloseHandle(handle)
        except Exception:  # pragma: no cover - best effort
            pass

    # ----------------------------------------------------------------- POSIX
    def _lock_path(self) -> Path:
        return Path(tempfile.gettempdir()) / f"{self.name}.lock"

    def _pid_path(self) -> Path:
        return Path(tempfile.gettempdir()) / f"{self.name}.pid"

    def _acquire_posix(self) -> None:
        fd = os.open(str(self._lock_path()), os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            os.close(fd)
            if exc.errno in (errno.EACCES, errno.EAGAIN):
                raise AlreadyRunning from exc
            raise
        self._lock_fd = fd
        self._pid_path().write_text(str(os.getpid()))

    def _read_pid(self) -> int | None:
        try:
            return int(self._pid_path().read_text().strip())
        except (OSError, ValueError):
            return None

    # ------------------------------------------------------------- release
    def release(self) -> None:
        if self._lock_fd is not None:
            try:
                fcntl.flock(self._lock_fd, fcntl.LOCK_UN)
                os.close(self._lock_fd)
                self._pid_path().unlink(missing_ok=True)
            except OSError:  # pragma: no cover
                pass
            self._lock_fd = None
        if self._mutex_handle is not None and IS_WINDOWS:
            self._close_handle(self._mutex_handle)
            self._mutex_handle = None


def _window_title() -> str:
    """Title used to locate the console window (kept in sync with app code)."""
    return os.environ.get("TRAYDROP_TITLE", "TrayDrop Console")
