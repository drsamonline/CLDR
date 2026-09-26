# TrayDrop

<p align="center">
  <img src="https://img.shields.io/badge/version-1.0.0-blue.svg" alt="Version"/>
  <img src="https://img.shields.io/badge/python-3.9%2B-green.svg" alt="Python"/>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey.svg" alt="Platform"/>
  <img src="https://img.shields.io/badge/license-MIT-orange.svg" alt="License"/>
  <img src="https://img.shields.io/badge/PRs-welcome-brightgreen.svg" alt="PRs Welcome"/>
</p>

> Run any console app **in the background**, keep it alive in the
> **notification tray**, and let a **desktop shortcut** summon the command
> window back to the foreground — without ever starting a second copy.

---

## ✨ Features

| Feature | How it works |
|---|---|
| 🖱️ **Launch via shortcut** | `scripts/launch_traydrop.bat` / `.vbs` (Windows), `launch_traydrop.sh` + `.desktop` entry (Linux) |
| 🤫 **Runs hidden in background** | Console window is hidden on start (`ShowWindow(SW_HIDE)` on Win32, minimised via `xdotool` elsewhere) |
| 📥 **Notification-tray icon** | [`pystray`](https://pypi.org/project/pystray/) icon drawn at runtime with Pillow — no binary assets needed |
| 🔁 **Shortcut re-open = bring window forward** | Named **mutex** (Windows) / **flock + SIGUSR1** (Linux) guarantees a single instance; the second launch raises the first instance's console and exits |
| 🧹 **Clean shutdown** | Tray → *Quit* stops the worker thread, removes the lock, releases the mutex |
| 🧩 **Bring-your-own workload** | Replace `DemoWorker.run()` with your real task loop |

## 🏗️ Architecture

```
Desktop Shortcut (.lnk / .desktop)
        │  double-click
        ▼
Launcher (bat/vbs/sh) ──► python -m traydrop
                              │
              ┌───────────────┴────────────────┐
              │ SingleInstance lock acquired?  │
              └───────┬───────────────┬────────┘
                    yes             no (2nd launch)
                      │               │
        hide console + tray icon      └─► SetForegroundWindow / SIGUSR1
        run DemoWorker thread              → existing window raised, exit
```

* **Windows:** `CreateMutexW("Local\TrayDropSingleInstance")`. A second
  process sees `ERROR_ALREADY_EXISTS`, calls `FindWindowW` +
  `ShowWindow(SW_RESTORE)` + `SetForegroundWindow` on the titled console, then quits.
* **Linux/macOS:** `flock` on `$TMPDIR/TrayDropSingleInstance.lock` plus a PID file;
  the second launch sends `SIGUSR1`, whose handler toggles the terminal window
  (needs `xdotool` under X11 for actual raise/hide).

## 📂 Project layout

```
.
├── src/traydrop/          # application package
│   ├── __init__.py        # version metadata
│   ├── __main__.py        # `python -m traydrop`
│   ├── app.py             # entry point, console REPL, worker wiring
│   ├── single_instance.py # mutex / flock + wake-up signalling
│   ├── tray.py            # pystray icon + menu
│   └── window.py          # hide/show/foreground helpers per OS
├── scripts/               # launchers & shortcut installers
├── docs/                  # USERGUIDE.md · SETUP.md · CHANGELOG.md …
├── tests/                 # unit tests (pytest)
├── pyproject.toml         # packaging metadata (PEP 621)
└── requirements.txt       # runtime dependencies
```

## 🚀 Quick start

```bash
git clone https://github.com/yourname/traydrop.git
cd traydrop
python -m venv .venv
# Windows: .venv\Scripts\activate     Linux/macOS: source .venv/bin/activate
pip install -e .
traydrop            # or: python -m traydrop
```

The window disappears into the tray ✅ — click the icon (or re-run the
shortcut) to bring it back. Full walkthrough: **[docs/SETUP.md](docs/SETUP.md)**.

## 💻 Console commands

| Command | Effect |
|---|---|
| `show` | Restore + foreground the command window |
| `hide` | Hide the window again (app keeps running) |
| `status` | Uptime + worker tick count |
| `quit` | Stop worker, leave tray, exit |

## 🧪 Development

```bash
pip install -e ".[dev]" || pip install pytest
pytest tests -q
```

## ⚠️ Known limitations

* Wayland does not allow one client to raise another's window — on Wayland
  the tray menu still works but shortcut-raising falls back to a notification.
* macOS has no native tray API in `pystray` without `rumps`; Windows/Linux are
  the supported targets.

## 📄 License

Released under the [MIT License](LICENSE). Free to use, modify and ship.

## 🙏 Acknowledgements

[`pystray`](https://github.com/moses-palmer/pystray) ·
[`Pillow`](https://python-pillow.org/) · Win32 `kernel32`/`user32`
