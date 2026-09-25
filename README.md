# CLDR (Command Line Dispatch & Route)

> **Minimal, zero-bloat, ultra-low-memory command launcher and background daemon for Windows and Linux.**

![Build](https://img.shields.io/github/actions/workflow/status/sohil-momin/cldr/build.yml?label=GitHub%20Build)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-blue)
![RAM](https://img.shields.io/badge/RAM-%3C10MB-brightgreen)
![License](https://img.shields.io/badge/License-MIT-yellow)

**Author:** Dr. Sohil Momin
**Language:** Rust 2021 · **UI:** crossterm + ratatui (raw terminal — no Electron, Tauri, Qt, or GTK)

---

## What is CLDR?

CLDR is a single-binary, instant-launch terminal command palette. It opens into a raw-mode
floating interface where every keystroke dispatches directly to the OS: spawn detached
processes, open files/folders in native viewers, walk bounded file trees, crunch arithmetic
with a zero-dependency parser, and route anything else intelligently — all while staying
under 10 MB of RAM.

An optional **background daemon** (`--daemon-start`) keeps a hidden, near-idle worker alive
to service queued dispatch requests (`open` / `run` / `sys`).

## Command Cheatsheet

| Command            | Description                                                        |
|--------------------|--------------------------------------------------------------------|
| `/run <binary>`    | Spawn a process **detached** via the OS shell (`cmd /C start` on Windows, `sh -c … &` on Linux). |
| `/open <path>`     | Open a file or folder in the system default viewer (`open::that()`). `~` expansion supported. |
| `/ls [path]`       | Instant non-blocking directory listing, capped at **50 items**, tagged `[DIR]` / `[FILE]`. |
| `/find <name>`     | Bounded recursive tree walk (`walkdir`, **max depth 4**) matching a filename substring. |
| `/web <query>`     | Search **DuckDuckGo** in the default browser.                       |
| `/calc <expr>`     | Deterministic zero-dependency arithmetic: `+ - * /`, parentheses, unary minus. |
| `/sys`             | Hardware diagnostics: OS family, architecture, CPU core count, hostname, user. |
| `/help`            | Show the cheat sheet inside the TUI.                                |
| `/clear`           | Clear the results pane.                                             |
| *(no prefix)*      | **Smart intent cascade**: try arithmetic → if it's a real path, open it → otherwise web-search it. |

### Key Bindings

| Key          | Action                                   |
|--------------|------------------------------------------|
| `Enter`      | Execute the current input                |
| `Up` / `Down`| Navigate the results list                |
| `PgUp`/`PgDn`| Jump 10 rows                             |
| `Home`/`End` | Top / bottom of results                  |
| `Esc`        | Clean exit — terminal state fully restored |

### Smart Intent Cascade Examples

| You type         | CLDR does                                        |
|------------------|--------------------------------------------------|
| `3*(4+2)`        | Evaluates to `18` (arithmetic first)             |
| `~/Documents`    | Opens the folder in the native file manager      |
| `rust lifetimes` | Falls back to a DuckDuckGo search                |

## CLI / Daemon Modes

```text
cldr                       launch interactive TUI
cldr --headless "<cmds>"   run without a TTY (newline-separated commands)
cldr --daemon              run hidden background worker (internal)
cldr --daemon-start        spawn the background daemon, fully detached
cldr --daemon-stop         signal a running daemon to shut down
cldr --daemon-status       report daemon liveness
cldr --notify "<request>"  queue a request for the daemon (open/run/sys)
cldr --version             print version
```

The daemon communicates through small sentinel files under `$XDG_RUNTIME_DIR`
(`%LOCALAPPDATA%` on Windows), heartbeats every tick, and exits within one second of a stop
request. Idle footprint is a sleeping thread plus two tiny file writes per tick.

## Build

```bash
cargo build --release
```

The release profile is tuned for size and speed:

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Run the binary:

```bash
./target/release/cldr            # Linux
.\target\release\cldr.exe        # Windows
```

## Architecture Notes

- **Single-file core engine** (`src/main.rs`) — dispatcher, parser, TUI, and daemon.
- **Memory ceiling by design**: results scrollback hard-capped at 5,000 rows; `/ls` capped at
  50 items; `/find` bounded to depth 4 with 50 matches; no async runtime, no GUI framework.
- **Detached spawning**: children never block or die with the UI.
- **Clean terminal restoration**: raw mode, alternate screen, mouse capture, and cursor state
  are always unwound on exit.

## License

Released under the **MIT License** — see [LICENSE](LICENSE).

© 2026 Dr. Sohil Momin
