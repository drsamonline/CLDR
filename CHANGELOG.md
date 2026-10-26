## 1.0.0 — 2026-10-25

- **Licence: PolyForm Noncommercial 1.0.0.** Every repository now carries the canonical text
  from polyformproject.org with the Required Notice line, and the four places a repo states
  its licence agree: LICENSE, the SPDX headers, the README and package.json.
- **The public demo is the app.** A build now fails if `docs/play/index.html` is not the app,
  because a demo running older code is two code paths for one program.

<!--
  CLDR — Command Line Dispatch & Route  ·  CHANGELOG
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  PolyForm Noncommercial-licensed: retain this attribution notice in all copies of this file.
-->

# Changelog

All notable changes to **CLDR** are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

[![Build & Release](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml/badge.svg)](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml)

---

## [Unreleased]

### Security
- The daemon no longer falls back to the bare, world-writable temp directory
  for its state/request queue. It now prefers `$XDG_RUNTIME_DIR`, then
  `~/.cache/cldr-daemon`, and only then a per-user temp sub-directory, and it
  tightens the directory to `0700` on Unix. Previously a system without
  `XDG_RUNTIME_DIR` could let any local user drop a `run`/`open` request that
  the daemon would execute as the victim.
- IPC control-channel reads (both client and server side) are now capped at
  256 bytes per line, so a rogue local connection cannot force unbounded
  allocation.

### Fixed
- The daemon's per-request outcome log was written to the heartbeat file,
  which is truncated on every tick — outcomes vanished within a second. They
  now go to a dedicated `outcomes.log`.
- `--exec`/`/calc` results with astronomically large or tiny magnitudes (e.g.
  `2^1000`) no longer print hundreds of digits; `fmt_num` falls back to
  compact scientific notation. `NaN`/`inf` are handled explicitly.
- The release workflow no longer attaches the stale, committed `dist/*`
  binaries to a release; fresh artifacts are staged in a dedicated directory.
- `install-shortcut.sh` only installs the systemd unit on a systemd host;
  `uninstall-shortcut.sh` prints the real state-dir path (fixed a `$$`/`${}`
  expansion bug).
- Docs: corrected version badges, installer output filenames, module map
  (`summon.rs` no longer described as a "clipboard mailbox"; `ipc.rs` added),
  and `.gitignore` now actually covers logs/OS/editor junk as CONTRIBUTING
  claims.
- IPC rendezvous cleanup now checks the recorded process PID before connecting
  and removes stale or unreachable `~/.cldr/port` records only when their
  observed `<port> <pid>` pair has not been replaced by a newer instance.
- `--notify` now writes atomically-published, per-request queue files instead
  of overwriting a single request file; rapid notifications are retained.
- Linux window raising now searches the terminal's ancestor PIDs and uses a
  PID-to-window-ID `wmctrl -lp` fallback when `xdotool` cannot activate it.

## [1.3.0] - 2026-10-21

### Added
- **Unit conversion** in the smart cascade: length, mass and temperature
  (`10 km in mi`, `100 f to c`, `5kg to lb`), with long unit names supported.
- **Config file** `~/.config/cldr/config.toml` (`%APPDATA%\cldr\config.toml`),
  parsed by a zero-dependency reader: configurable `search_url`, `max_results`,
  `find_depth`, and `alias.<name>` command aliases.
- **Command history** — `~`/`down`, `Ctrl+P`/`Ctrl+N`, persisted to
  `~/.config/cldr/history` (capped, de-duplicated).
- **`Ctrl+Y` copy** of the last result via OSC 52 (no clipboard crate, works
  over SSH); **`Ctrl+C`** quits; **`Ctrl+L`** clears the pane.
- **`cldr --self-test`** — 14 headless checks for CI on terminal-less runners.
- **`cldr --config`** — write/print the starter config file.
- **Compact window on Windows**: the console is shrunk to a ~92x26 box and
  positioned near the top of the screen on startup.
- **`packaging/`** — Inno Setup Windows installer, Linux `install.sh` /
  `uninstall.sh`, portable notes; **`rust-toolchain.toml`**, **`.gitattributes`**,
  **`SECURITY.md`**.
- **Docs**: rendered UI previews under `docs/screenshots/`, beautified README.

### Changed
- **Performance**: the TUI now redraws only when something changes (dirty flag)
  instead of every 16 ms; idle cost is a single 250 ms `poll()`.
- Release profile tightened for the smallest + fastest binary: full LTO, one
  codegen unit, stripped symbols, `panic=abort`, no overflow checks.
- **CI**: tag-driven releases (`git tag v1.3.0 && git push origin v1.3.0`);
  portable archives for Windows + Linux (x86_64, aarch64), a Windows installer,
  `SHA256SUMS.txt`, and SLSA build provenance. `contents: write` is scoped to
  the publish job only. `rustfmt`/`clippy` run as informational gates.

## [1.2.0] - 2026-09-26

### Added
- **A1 — In-process IPC control channel** (`src/ipc.rs`): loopback-TCP
  summon/PING protocol (`SUMMON`, `PING` → `OK-WINDOW`, `OK-TRAY`, `PONG`)
  with port+PID rendezvous at `~/.cldr/port`. Zero new crates (`std::net`).
- **A3 — Single-instance guard**: launching `cldr` while a window is live
  now summons that window and exits in <50 ms instead of spawning a
  duplicate (RAM + tray sanity).
- Richer `--daemon-status`: reports daemon *and* window liveness
  (`[daemon alive · window live]`).
- Two new unit tests covering the IPC round-trip and error replies
  (8 tests total, all passing).

### Changed
- `--summon` routes through the IPC channel; if only the tray answers
  (`OK-TRAY`) or nobody answers, a fresh command window is opened.
- The TUI registers as a *window instance* so SUMMON acks carry raise-focus
  semantics; deregisters its rendezvous file on clean exit.
- Docs updated to describe the IPC architecture (README sequence diagram,
  SHORTCUTS "IPC control-channel notes", SETUP_GUIDE, USER_GUIDE).

### Removed
- **Clipboard mailbox deleted entirely** — the app no longer reads or writes
  your clipboard ever again (privacy + reliability win).
- Dropped the `arboard` dependency (~−40 KB transitive weight; binary stays
  ~830 KB stripped).

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
PolyForm Noncommercial licensed · attribution watermark must be retained in all copies.

</div>
