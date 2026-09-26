# 📖 TrayDrop — User Guide

[← Back to README](../README.md) · [Setup Guide](SETUP.md)

---

## 1. The idea in one paragraph

You double-click the **TrayDrop shortcut**. A command window flashes for a
moment, then vanishes — the app is now *running in the background* with a small
terminal icon in your **notification tray** (system tray). Later, you
double-click the same shortcut again: instead of opening a duplicate app,
Windows/Linux simply **brings the original command window back to the
foreground**. That's it. One instance, always reachable.

## 2. First launch

1. Make sure setup is complete ([docs/SETUP.md](SETUP.md)).
2. Double-click **TrayDrop** on the Desktop (or run `traydrop` in a terminal).
3. Look for the blue ▸_ icon in the tray. Hover it:
   *"TrayDrop – running in background (click to show console)"*.
4. You'll get a balloon notification confirming background mode.

> 💡 On Windows the launcher uses `pythonw`/minimized start so there is little
> or no window flash at all.

## 3. Working with the tray icon

| Action | Result |
|---|---|
| **Left click / double-click** | Bring the command window forward |
| Right-click → **Show Console** | Restore + focus the window |
| Right-click → **Hide Console** | Send the window back to the tray |
| Right-click → **About TrayDrop** | Version balloon |
| Right-click → **Quit** | Stop the worker and exit cleanly |

## 4. Using the shortcut to re-open the window

Just double-click the shortcut again while the app is running:

* **Windows** — the second process detects the named mutex, calls
  `SetForegroundWindow` on the window titled *"TrayDrop Console"* and exits.
* **Linux (X11)** — the second process sends `SIGUSR1` to the PID stored in
  `/tmp/TrayDropSingleInstance.pid`; the running app raises its terminal via
  `xdotool`. Install `xdotool` for full behaviour.

If you ever see *"TrayDrop is already running"* printed briefly — that's the
wake-up stub doing its job.

## 5. The console window

The window is a normal command prompt with a tiny REPL:

```
=== TrayDrop v1.0.0 ===
Commands: show | hide | status | quit
TrayDrop> status
running 42s, ticks=8
TrayDrop> hide
```

Your own workload prints into this window too — replace `DemoWorker.run()`
(see §7) and everything your app logs appears here, even while hidden.

## 6. Stopping the app

Preferred: tray icon → **Quit**. Alternatives:

* type `quit` in the console window, or
* Task Manager → end `python`/`pythonw` process (lock file is auto-cleaned on
  next launch since `flock` dies with the process).

## 7. Plugging in *your* program

Open [`src/traydrop/app.py`](../src/traydrop/app.py) and edit `DemoWorker`:

```python
class DemoWorker:
    def run(self):
        while not self.stop_event.is_set():
            do_your_real_work()          # <-- your code here
            self.stop_event.wait(5.0)    # polite pause / retry backoff
```

Rules of thumb:

* ✅ Check `self.stop_event.is_set()` inside long loops so **Quit** works fast.
* ✅ Print freely — output goes to the console window.
* ❌ Don't call `sys.exit()` from the worker; set logic to return instead.

## 8. Autostart with login

* **Windows:** copy the shortcut into `shell:startup`
  (Win+R → `shell:startup`). `make_shortcut.ps1` has a commented block for it.
* **Linux:** `cp scripts/traydrop.desktop ~/.config/autostart/`

## 9. Troubleshooting

| Symptom | Fix |
|---|---|
| No tray icon appears | Install deps: `pip install -r requirements.txt`. Some minimal Linux sessions lack a StatusNotifier host — try GNOME/KDE, or run visibly. |
| Shortcut opens a *second* copy | Delete stale lock: remove `%TEMP%\TrayDropSingleInstance.*` (Windows) or `/tmp/TrayDropSingleInstance.*` (Linux), then relaunch. On Windows make sure both copies use the same Python (`py -3`). |
| Window won't come forward (Linux) | You're likely on Wayland — install an X11 session or use the tray menu's *Show Console*. Ensure `xdotool` is installed. |
| "traydrop is not recognized" | Run `pip install -e .` inside your activated virtual environment, or use `python -m traydrop`. |
| App killed when closing the flashing window | Use the `.vbs`/`pythonw` launcher path so no disposable cmd window owns the process. |

---

*Found this guide useful? Give the repo a ⭐ — contributions welcome (see
[CONTRIBUTING.md](CONTRIBUTING.md)).*
