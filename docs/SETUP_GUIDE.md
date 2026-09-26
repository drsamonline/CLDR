<!--
  CLDR — Command Line Dispatch & Route  ·  SETUP GUIDE
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  MIT-licensed: THE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN ALL
  COPIES OR SUBSTANTIAL PORTIONS OF THIS DOCUMENT. Do not remove this watermark.
-->

<div align="center">

# 🛠️ CLDR — Setup Guide

[![Build & Release](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml/badge.svg)](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml)
![Version](https://img.shields.io/badge/version-1.1.0-blue?style=flat-square)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue?style=flat-square)

*← [README](../README.md) · [User Guide](USER_GUIDE.md) · [Shortcut recipes](SHORTCUTS.md)*

</div>

---

## Table of Contents

| Section | Audience |
|---|---|
| [1 · Prerequisites](#1--prerequisites) | everyone |
| [2 · Build from source](#2--build-from-source) | developers |
| [3 · Prebuilt binaries](#3--prebuilt-binaries) | users |
| [4 · Install the launch shortcut](#4--install-the-launch-shortcut) | everyone |
| [5 · Background + tray autostart](#5--background--tray-autostart) | everyone |
| [6 · Bind a global hotkey](#6--bind-a-global-hotkey) | everyone |
| [7 · Verify your install](#7--verify-your-install) | everyone |
| [8 · Uninstall](#8--uninstall) | everyone |
| [9 · Releasing (maintainers)](#9--releasing-maintainers) | maintainers |

---

## 1 · Prerequisites

| Requirement | Windows | Linux |
|---|---|---|
| Rust toolchain (build only) | [rustup](https://rustup.rs) → MSVC | `curl https://sh.rustup.rs -sSf \| sh` |
| Terminal | **Windows Terminal** (recommended) or conhost | any |
| Optional window raiser | — | `xdotool` *or* `wmctrl` (`sudo apt install xdotool`) |
| Optional hotkey daemon | PowerToys / AutoHotkey | `sxhkd` (i3/Sway: own binds) |

No other runtime dependencies — CLDR is a single static-ish binary (<10 MB RAM).

## 2 · Build from source

```bash
git clone https://github.com/drsamonline/cldr.git
cd cldr
cargo build --release          # binary → target/release/cldr(.exe)
cargo test                     # 6 engine unit tests must pass
```

<details>
<summary><b>Cross-compiling targets used by CI</b></summary>

| Target | Command |
|---|---|
| `x86_64-unknown-linux-gnu` | `cargo build --release --locked --target x86_64-unknown-linux-gnu` |
| `aarch64-unknown-linux-gnu` | needs `gcc-aarch64-linux-gnu`, linker env var (see workflow) |
| `x86_64-pc-windows-msvc` | run on Windows or via `cargo-xwin` |

</details>

## 3 · Prebuilt binaries

Grab the archive for your platform from the
[Releases page](https://github.com/drsamonline/cldr/releases):

| Archive | Platform |
|---|---|
| `cldr-windows-x86_64-msvc.zip` | Windows 10/11 x64 |
| `cldr-linux-x86_64.tar.gz` | Linux x86_64 (glibc ≥ 2.31) |
| `cldr-linux-aarch64.tar.gz` | Linux ARM64 (Pi 4/5, RK boards) |

```powershell
# Windows (PowerShell)
Expand-Archive cldr-windows-x86_64-msvc.zip -DestinationPath $env:LOCALAPPDATA\cldr
```

```bash
# Linux
tar -xzf cldr-linux-x86_64.tar.gz -C ~/.local/bin/   # creates ~/.local/bin/cldr
```

## 4 · Install the launch shortcut

This is the step that **binds the app to a launching shortcut**, keeps a
**background tray-resident process**, and makes the shortcut **bring the command
window forward**.

### Windows

```powershell
cd cldr
.\scripts\install-shortcut.ps1            # add -BinPath "D:\tools\cldr.exe" if elsewhere
```

What it creates:

| Item | Location | Action |
|---|---|---|
| 🖥️ Launch shortcut | Start Menu → *CLDR* | opens the command window |
| ⚡ Summon shortcut | Desktop → *Summon CLDR* | brings the running window forward (`--summon`) |
| 🔄 Tray autostart | `%STARTUP%` → *CLDR Tray* (hidden) | runs `wscript summon.vbs` at logon → background `--tray` |

### Linux

```bash
./scripts/install-shortcut.sh             # honours $BIN override
```

Creates `~/.local/bin/cldr`, an application-menu entry, an
**XDG autostart** tray entry (`~/.config/autostart/cldr-tray.desktop`), a
systemd user service (`systemctl --user enable cldr`), and — if `sxhkd` is
present — a `Super+Alt+C → cldr --summon` hotkey line.

## 5 · Background + tray autostart

Two supported residency models (pick one; both are installed by default):

1. **Tray loop (`cldr --tray`)** — a lightweight background process that keeps
   the daemon alive and serves the loopback IPC control channel for summons.
   Started automatically at logon by the Startup folder / XDG autostart entry.
2. **On-demand daemon (`cldr --daemon-start`)** — detached request worker with
   PID/heartbeat files under `~/.cldr/`. Status: `cldr --daemon-status`,
   stop: `cldr --daemon-stop`.

## 6 · Bind a global hotkey

Per-DE / per-tool recipes (KDE, GNOME, i3/Sway, Hyprland, PowerToys, AutoHotkey)
live in **[docs/SHORTCUTS.md](SHORTCUTS.md)**. The universal command to bind is:

```
cldr --summon
```

## 7 · Verify your install

```bash
cldr --version        # → cldr 1.1.0
cldr --exec "2^10"    # → [CALC] = 1024  (headless engine check)
cldr --daemon-status  # shows PID / heartbeat of background worker
```

Then press your bound hotkey while no window is open — a command window should
appear; press it again with the window open — it should be raised to the
foreground.

## 8 · Uninstall

```bash
./scripts/uninstall-shortcut.sh           # Linux: removes bin link, .desktop, autostart, service, sxhkd line
```

```powershell
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\CLDR*.lnk"
Remove-Item "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\CLDR*.lnk"
Remove-Item "$env:LOCALAPPDATA\cldr" -Recurse
```

## 9 · Releasing (maintainers)

1. Bump `version` in `Cargo.toml` **and** add a `CHANGELOG.md` entry.
2. Commit, then tag exactly: `git tag v<version> && git push origin v<version>`.
3. The [`build-release.yml`](../.github/workflows/build-release.yml) workflow
   builds all three targets, **verifies the tag matches Cargo.toml**, and
   publishes a GitHub Release with the binaries attached.

---

<div align="center">

**© 2026 Dr. Sohil Momin ([@drsamonline](https://github.com/drsamonline))** —
MIT licensed · attribution watermark must be retained in all copies.

</div>
