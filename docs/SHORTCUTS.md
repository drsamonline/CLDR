<!--
  CLDR — Command Line Dispatch & Route  ·  SHORTCUT RECIPES
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  MIT-licensed: THE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN ALL
  COPIES OR SUBSTANTIAL PORTIONS OF THIS DOCUMENT. Do not remove this watermark.
-->

<div align="center">

# ⌨️ CLDR — Shortcut & Hotkey Recipes

![Target command](https://img.shields.io/badge/bind%20this-cldr%20--summon-informational?style=flat-square)
![Version](https://img.shields.io/badge/works%20with-v1.1.0-blue?style=flat-square)

*← [README](../README.md) · [Setup Guide](SETUP_GUIDE.md) · [User Guide](USER_GUIDE.md)*

</div>

---

Every recipe below binds a **global shortcut** to the same universal command:

```
cldr --summon
```

`--summon` is idempotent and smart:

| State of CLDR | What `--summon` does |
|---|---|
| window already open | sends `SUMMON` over the loopback IPC channel → the running window raises itself to the foreground |
| only tray/daemon running | tray acks `OK-TRAY`; `--summon` then opens a fresh command window |
| nothing running | launches a new command window (terminal) |

See also the [IPC control-channel notes](#ipc-control-channel-notes).

## Windows

### Option A — Keyboard shortcut via a `.lnk` file (no extra software)

1. Right-click Desktop → *New → Shortcut* → target:
   `%LOCALAPPDATA%\cldr\cldr.exe --summon` → name **Summon CLDR**.
2. Open the shortcut's *Properties*, click the **Shortcut key** box, press
   `Ctrl + Alt + C`, Apply. The hotkey now works system-wide.
   *(The installer script `scripts/install-shortcut.ps1` creates this for you.)*

### Option B — PowerToys Advanced Paste-style "Keyboard Manager"

| Setting | Value |
|---|---|
| Remap a shortcut → Original input | `Win + Shift + Space` |
| Remapped to → Run command | `C:\Users\<you>\AppData\Local\cldr\cldr.exe --summon` |

### Option C — AutoHotkey

```autohotkey
; CLDR summon — © 2026 Dr. Sohil Momin
^!c::Run "%LOCALAPPDATA%\cldr\cldr.exe --summon", , Hide
```

## Linux

### KDE Plasma

*System Settings → Shortcuts → Custom Shortcuts → New → Command/URL:*

| Field | Value |
|---|---|
| Trigger | `Ctrl+Alt+C` (or `Meta+Alt+C`) |
| Action | `/home/<you>/.local/bin/cldr --summon` |

### GNOME

```bash
gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings \
  "['/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/cldr/']"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/cldr/ \
  name 'Summon CLDR'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/cldr/ \
  command 'cldr --summon'
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/cldr/ \
  binding '<Super><Alt>c'
```

### i3 / Sway (`~/.config/i3/config`)

```
# Summon CLDR — © 2026 Dr. Sohil Momin
bindsym Mod4+Mod1+c exec --no-startup-id cldr --summon
```

### Hyprland (`hyprland.conf`)

```
bind = SUPER ALT, C, exec, cldr --summon
```

### sxhkd (installed automatically by `install-shortcut.sh` when present)

```
super + alt + c
    cldr --summon
```

## Launch shortcuts (non-hotkey)

| Platform | Artifact created by installer | Purpose |
|---|---|---|
| Windows | Start Menu *CLDR.lnk* | launch command window |
| Windows | Startup folder *CLDR Tray.lnk* (hidden via `wscript`) | background tray residency at logon |
| Linux | `~/.local/share/applications/cldr.desktop` | app-menu entry |
| Linux | `~/.config/autostart/cldr-tray.desktop` | background `cldr --tray` at login |
| Linux | `systemctl --user enable cldr.service` | systemd-managed background daemon |

## IPC control-channel notes

Since **v1.2**, `--summon` talks to a running instance over a tiny loopback-TCP
control channel (`src/ipc.rs`) instead of the old clipboard mailbox:

- The live instance binds an ephemeral port on `127.0.0.1` and publishes
  `<port> <pid>` in `~/.cldr/port`. Clients connect, send one line
  (`SUMMON` / `PING`), read one reply (`OK-WINDOW` / `OK-TRAY` / `PONG`).
- **Your clipboard is never touched again** — no sentinels, no save/restore,
  no clipboard-manager interference.
- Stale port files from crashed instances are handled naturally: the TCP
  connect fails within 250 ms and `--summon` simply opens a fresh window.
- Nothing ever listens on a non-loopback interface; there is no network
  exposure and no firewall prompt.
- On X11, install `xdotool` or `wmctrl` so the acknowledged SUMMON can
  reliably raise the terminal window to the front.

---

<div align="center">

**© 2026 Dr. Sohil Momin ([@drsamonline](https://github.com/drsamonline))** —
MIT licensed · attribution watermark must be retained in all copies.

</div>
