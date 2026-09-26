<!--
  CLDR — Command Line Dispatch & Route  ·  CHANGELOG
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  MIT-licensed: retain this attribution notice in all copies of this file.
-->

# Changelog

All notable changes to **CLDR** are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

[![Build & Release](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml/badge.svg)](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml)

---

## [1.1.0] — 2026-09-26

### Added
- **Exponentiation operator `^`** in the calculator (right-associative, unary
  exponents supported: `2^3^2 = 512`, `2^-1 = 0.5`) with a new unit test.

### Added
- **Background tray residency** — `cldr --tray` keeps a lightweight watcher
  alive; autostart entries installed by the shortcut scripts (`--tray` on XDG
  autostart / Windows Startup folder, hidden via `wscript`).
- **Shortcut summon** — `cldr --summon` brings an open command window to the
  foreground (Win32 `AttachThreadInput` raise on Windows, `xdotool`/`wmctrl` on
  Linux) or launches a new one; clipboard-mailbox sentinel protocol between the
  background process and the TUI (20 Hz watcher thread).
- **Launch-shortcut installers** — `scripts/install-shortcut.ps1` (Start Menu +
  Desktop Summon + hidden Startup tray entry + `summon.vbs`) and
  `scripts/install-shortcut.sh` (bin symlink, app-menu `.desktop`, XDG
  autostart, sxhkd hotkey line, systemd user service), plus
  `scripts/uninstall-shortcut.sh`.
- **Headless CLI modes** — `--exec "<input>"` (dispatch without UI),
  `--daemon-start / --daemon-stop / --daemon-status`, `--notify`.
- **PATH-binary step in the smart cascade** — unmatched inputs now fall through
  to your shell's PATH before the web-search default.
- **Documentation suite** — rewritten `README.md`, `docs/USER_GUIDE.md`,
  `docs/SETUP_GUIDE.md`, `docs/SHORTCUTS.md`, `CONTRIBUTING.md`; Mermaid summon
  sequence diagram; shields.io/GitHub badges throughout.
- **CI/CD** — single canonical `.github/workflows/build-release.yml`: build +
  test gate on Ubuntu/Windows, release matrix (linux-x86_64, linux-aarch64,
  windows-msvc), tag↔Cargo.toml version guard, GitHub Release with binaries.
- **Attribution watermark** — `© 2026 Dr. Sohil Momin (drsamonline)` banner in
  every source, doc, script, workflow, and asset file; `LICENSE` upgraded to
  MIT-with-mandatory-attribution.

### Changed
- Split the 1,200-line monolith `src/main.rs` into modules: `engine`, `daemon`,
  `tui`, `summon`, plus a thin CLI router `main.rs`.
- `Cargo.toml`: version 1.0.x → 1.1.0, added repository/homepage/documentation
  metadata, keywords/categories; removed unused heavy GUI deps
  (taffy/tray-icon/muda/tao/windows-sys) that caused `gtk-sys` conflicts;
  added `arboard` for the clipboard mailbox.

### Removed
- Dead code and clutter: stray duplicate workflows (`workflows/release.yml`,
  `.github/workflows/RELEASE.YML`), leftover scratch `PROMPT.txt`, empty
  `.gitignore` replaced with a real one (`/target`, logs, OS junk files).

## [1.0.0] — 2026-09-20

### Added
- Initial release: ratatui command-palette TUI, expression calculator, unit
  conversions, URL encoding, web-search fallback, slash-command dispatcher,
  `open` integration, background request daemon prototype.

---

<div align="center">

**© 2026 Dr. Sohil Momin ([@drsamonline](https://github.com/drsamonline))** —
MIT licensed · attribution watermark must be retained in all copies.

</div>
