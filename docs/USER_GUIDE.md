<!--
  CLDR — Command Line Dispatch & Route · USER GUIDE
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  PolyForm Noncommercial-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
  ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this watermark
  is a violation of the license terms.
-->

# 📖 CLDR User Guide

[![Build](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml/badge.svg)](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml)
· ← back to [README](../README.md) · see also [Setup Guide](SETUP_GUIDE.md) · [Shortcut recipes](SHORTCUTS.md)

---

## 1. The mental model

CLDR has **three cooperating pieces**:

| Piece | Flag | Lives where | Job |
|---|---|---|---|
| **Command window** | *(none)* / `--summon` | your terminal | interactive palette (type → route) |
| **Tray resident** | `--tray` | background, notification area | keeps everything alive, autostarts, respawns daemon |
| **Dispatch daemon** | `--daemon-start` | detached worker | services queued `open`/`run`/`sys` requests from durable per-request files |

You normally never touch the daemon directly — the shortcut installer wires all three up.

## 2. Launching & summoning

1. Double-click the **CLDR** shortcut (or run `cldr`) → command window opens.
2. Press your **summon hotkey** or the **CLDR Summon** shortcut again →
   *the existing window is brought forward and focused* instead of opening a duplicate.
3. Minimize/close the window? The tray process (`--tray`) stays resident; the next
   summon re-focuses it.
4. Reboot → the Startup / XDG-autostart entry relaunches `cldr --tray` silently.

> 💡 Windows: `Win+Alt+C` (via PowerToys/AutoHotKey bound to `%LOCALAPPDATA%\cldr\summon.vbs`).
> Linux: `Super+Alt+C` (installed automatically if `sxhkd` is present).

## 3. Typing — the smart cascade

Anything **without** a leading `/` is auto-routed:

```text
math expression?      → compute inline          e.g.  2*(3+4)/7   → 2
unit conversion?      → convert                 e.g.  10 km in mi  → 6.213712 mi
existing file/folder? → open in default viewer  e.g.  ~/Downloads
on PATH as a binary?  → spawn detached          e.g.  code .
otherwise?            → web search              e.g.  rust lifetimes
```

Unit conversion understands length (`mm cm m km in ft yd mi nmi`), mass
(`mg g kg oz lb st t`) and temperature (`c f k`), and accepts long names
(`10 kilometres in miles`).

## 4. Slash commands

| Command | Example | Result |
|---|---|---|
| `/run <cmd>` | `/run notepad` | detached spawn via OS shell |
| `/open <path>` | `/open C:\Temp` | native "open with default" |
| `/ls [path]` | `/ls ~/` | directory listing (≤ 50 rows) |
| `/find <name>` | `/find report.pdf` | recursive search, depth ≤ 4, ≤ 50 hits |
| `/web <query>` | `/web cat facts` | browser search |
| `/calc <expr>` | `/calc (1+2)*3^…` | arithmetic `+ - * / % ^ ( ) unary-` |
| `/sys` | `/sys` | cores, RAM, hostname, user |
| `/help` | | cheat sheet in-pane |
| `/clear` | | clear results pane |

Paths accept `~`, `~/sub`, absolute and relative forms on both platforms.

## 5. Keyboard reference (in-window)

| Key | Action |
|---|---|
| `Enter` | submit line |
| `↑` `↓` | recall history when the input is empty, otherwise scroll results |
| `Ctrl+P` `Ctrl+N` | history prev / next (always) |
| `PgUp` `PgDn` `Home` `End` | scroll results |
| `Ctrl+Y` | copy the last result to the clipboard (OSC 52 — works over SSH) |
| `Ctrl+L` | clear the results pane |
| `Del` | clear the input buffer |
| `Esc` / `Ctrl+C` | quit (terminal restored cleanly) |
| printable + `Shift` | normal text entry |

Status strip shows last outcome + current working directory. Scrollback is hard-capped
at 5 000 lines so memory can never grow unbounded.

## 6. Headless / scripting modes

```bash
cldr --exec "12*3.5"              # prints "[CALC] 12*3.5 = 42", exits — great for shortcuts
cldr --exec "10 km in mi"         # one-shot unit conversion
cldr --self-test                  # 14 headless checks, exit 0 = pass
cldr --config                     # write/print the config file path
cldr --notify "open ~/notes.md"   # queue work for the background daemon
cldr --daemon-status              # alive? stopped?
cldr --daemon-stop                # graceful shutdown (sentinel file)
```

## 6a. Configuration & aliases

`cldr --config` creates `~/.config/cldr/config.toml`
(`%APPDATA%\cldr\config.toml` on Windows). Everything is optional:

```toml
search_url = "https://duckduckgo.com/?q={q}"   # change the search engine
max_results = 50                                 # /ls and /find row cap
find_depth  = 4                                  # /find recursion depth
alias.g  = /web                                  # "g rust" → "/web rust"
alias.n  = /run nvim
```

Command history is stored alongside it in `~/.config/cldr/history` (last 500
commands, de-duplicated).

Use `--exec` to build extra one-shot shortcuts (e.g. a desktop icon that runs
`cldr --exec "/open ~/Documents"`).

## 7. Troubleshooting

| Symptom | Fix |
|---|---|
| Summon does nothing | ensure a CLDR window is actually running; stale `~/.cldr/port` records self-heal by checking the recorded PID, or after one 250 ms unreachable-port attempt (see [SHORTCUTS.md](SHORTCUTS.md#ipc-control-channel-notes)) |
| Hotkey dead on Linux | no `sxhkd`? bind manually in KDE/GNOME settings to `cldr --summon` ([recipes](SHORTCUTS.md)) |
| Garbled terminal after crash | run `reset` — CLDR always restores on clean exit (`Esc`) |
| Daemon won't start | check `$XDG_RUNTIME_DIR/cldr-daemon/daemon.log` (Linux) or `%LOCALAPPDATA%\cldr-daemon\daemon.log` (Windows); outcomes land in `outcomes.log` next to it |
| Ctrl+Y didn't copy | your terminal may block OSC 52 — most (Windows Terminal, GNOME Terminal, kitty, alacritty) allow it; check terminal settings |
| Autostart missing | rerun the installer script; on Windows sign out/in once for the Startup folder entry |

---

*© 2026 Dr. Sohil Momin — keep this attribution in any copy.*
