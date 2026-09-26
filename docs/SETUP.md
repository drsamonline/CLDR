# 🛠️ TrayDrop — Setup Guide

[← Back to README](../README.md) · [User Guide](USERGUIDE.md)

---

## ✅ Prerequisites

| Requirement | Version | Check with |
|---|---|---|
| Python | 3.9+ | `python --version` |
| pip | current | `python -m pip --version` |
| OS | Windows 10/11 *(primary)*, Linux (X11) | — |
| xdotool *(Linux only, optional)* | any | `xdotool version` |

---

## 1 · Clone & install

```bash
git clone https://github.com/yourname/traydrop.git
cd traydrop

python -m venv .venv
# Windows:
.venv\Scripts\activate
# Linux/macOS:
source .venv/bin/activate

pip install -e .          # installs traydrop + pystray + Pillow
```

Smoke test (window will hide into the tray):

```bash
python -m traydrop
```

---

## 2 · Bind a launching shortcut

### 🪟 Windows (recommended path)

Option A — one-click script:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\make_shortcut.ps1
```

Creates **TrayDrop.lnk** on your Desktop pointing at
`scripts\launch_traydrop_silent.vbs`. Double-click it to start; double-click
again to bring the command window forward.

Option B — manual shortcut:

1. Right-click Desktop → **New → Shortcut**.
2. Location:
   * silent: `wscript.exe "C:\path\to\traydrop\scripts\launch_traydrop_silent.vbs"`
   * or simple: `"C:\path\to\traydrop\scripts\launch_traydrop.bat"`
3. Name it **TrayDrop**, finish.
4. (Nice-to-have) Properties → *Run: Minimized*, and change the icon
   (`shell32.dll,172`).

> 🔒 The magic that makes "click again = open existing window" work is the
> named mutex inside `traydrop.single_instance` — no extra configuration needed.

### 🐧 Linux (X11 desktops: GNOME, KDE, XFCE…)

```bash
chmod +x scripts/launch_traydrop.sh
sudo apt install xdotool            # enables raise/minimise of the terminal

mkdir -p ~/.local/share/applications
cp scripts/traydrop.desktop ~/.local/share/applications/
update-desktop-database ~/.local/share/applications || true
```

Edit the `Exec=` line in `~/.local/share/applications/traydrop.desktop` so it
points at your checkout, e.g.:

```ini
Exec=/bin/bash -c "cd /home/you/traydrop && ./scripts/launch_traydrop.sh"
```

Then drag **TrayDrop** from your app grid to the Desktop/Dock — that's your
launcher. Choose a terminal via env var if desired:
`TERM_LAUNCHER=gnome-terminal` (or `konsole`, `xterm`).

---

## 3 · Auto-start on login (optional)

* **Windows:** Win+R → `shell:startup` → copy `TrayDrop.lnk` there
  (or uncomment the block at the end of `make_shortcut.ps1`).
* **Linux:** `cp ~/.local/share/applications/traydrop.desktop ~/.config/autostart/`

---

## 4 · Verify your installation

- [ ] `python -m traydrop` starts and the console hides within ~1 s
- [ ] Tray icon ▸_ appears; hover shows the tooltip
- [ ] Tray → *Show Console* restores the window
- [ ] Re-running the shortcut raises the same window (no second process)
- [ ] Tray → *Quit* removes the icon and exits

Run the automated checks too:

```bash
pip install pytest
pytest tests -q
```

---

## 5 · Uninstall

```bash
pip uninstall traydrop
rm ~/.local/share/applications/traydrop.desktop   # Linux
del "%USERPROFILE%\Desktop\TrayDrop.lnk"          # Windows
```

Lock/pid files live in the system temp dir and are cleaned automatically.

---

## ❓ Problems?

See the troubleshooting table in the [User Guide §9](USERGUIDE.md#9-troubleshooting)
or [open an issue](https://github.com/yourname/traydrop/issues).
