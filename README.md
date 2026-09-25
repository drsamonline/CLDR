# CLDR (Command Line Dispatch & Route)

[![GitHub Build](https://img.shields.io/github/actions/workflow/status/sohilmomin/cldr/build.yml?label=Build&logo=github)](https://github.com/sohilmomin/cldr/actions)
![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-blue?logo=linux)
![RAM Usage](https://img.shields.io/badge/RAM%20Usage-%3C10MB-brightgreen)
![License](https://img.shields.io/badge/License-MIT-yellow.svg)

> **Author:** Dr. Sohil Momin
> **Language:** Rust 2021 — zero-bloat, ultra-low memory command launcher and background daemon.

CLDR is a minimal, single-binary terminal launcher. It boots instantly into a raw-mode
floating TUI (no Electron / Tauri / Qt / GTK), dispatches commands through the native OS
shell, and routes plain input through a smart intent cascade: **math → file path → web search**.

---

## Features

- **Instant raw-terminal UI** — `crossterm` + `ratatui` alternate-screen interface with
  top input bar, navigable results list, and bottom status strip.
- **Detached process launching** — `/run` spawns via `cmd /C start` (Windows) or
  `sh -c '... &'` (Linux) with fully nulled stdio.
- **Native OS dispatch** — files, folders, and URLs open through the system default
  handler via `open::that()`.
- **Bounded filesystem search** — `/find` performs a depth-limited (`max-depth = 4`)
  recursive walk using `walkdir`, capped at 50 results for instant response.
- **Zero-dependency calculator** — deterministic recursive-descent parser for `+ - * /`
  with parentheses, unary signs, decimals, and division-by-zero safety (never panics).
- **Smart intent cascade** — input without `/` is evaluated as arithmetic first; if it
  matches an existing path on disk it is opened; otherwise it becomes a DuckDuckGo query.
- **Tiny footprint** — stripped release binary (~750 KB) with LTO + `codegen-units = 1`;
  measured working set under 1 MB RSS at idle (comfortably below the 10 MB budget).

---

## Command Cheatsheet

| Command | Syntax | Description |
|---|---|---|
| `/run` | `/run <binary> [args]` | Launch a detached process via the OS shell (`cmd /C start` on Windows, `sh -c` on Linux). |
| `/open` | `/open <path>` | Open a file or folder in the system default viewer (`open::that()`). Supports `~` expansion. |
| `/ls` | `/ls [path]` | Instant directory listing tagged `[DIR]` / `[FILE]`, capped at 50 items. Defaults to cwd. |
| `/find` | `/find <name>` | Bounded recursive file-tree walk (max depth 4), case-insensitive substring match, capped at 50 hits. |
| `/web` | `/web <query>` | Open a DuckDuckGo search in the default browser. |
| `/calc` | `/calc <expr>` | Arithmetic evaluator: `+ - * / ( )`, e.g. `/calc (2+3)*4/2` → `10`. |
| `/sys` | `/sys` | Hardware diagnostics: architecture, OS family, CPU core count, host, cwd. |
| `/help` | `/help` | Show the cheatsheet inside the TUI. |
| `/clear` | `/clear` | Clear the results view. |
| *(no prefix)* | `2+2` · `~/Docs` · `rust lang` | Smart cascade: evaluate math → open existing path → fall back to web search. |

### Key Bindings

| Key | Action |
|---|---|
| `Enter` | Execute current input |
| `Up` / `Down` | Navigate the results list |
| `Esc` (or `Ctrl+C`) | Cleanly exit and restore terminal state |

---

## Build

Requires a stable Rust 2021 toolchain (1.74+).

```bash
git clone https://github.com/sohilmomin/cldr.git
cd cldr
cargo build --release
```

The optimized binary is emitted to:

- **Linux:** `target/release/cldr`
- **Windows:** `target\release\cldr.exe`

Run it:

```bash
./target/release/cldr        # Linux
target\release\cldr.exe      # Windows
```

### Release Profile (already configured in `Cargo.toml`)

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

### Run the test suite

```bash
cargo test
```

---

## Project Layout

```
cldr/
├── Cargo.toml      # deps: crossterm 0.28, ratatui 0.29, open 5.3, walkdir 2.5
├── LICENSE         # MIT (Dr. Sohil Momin)
├── README.md
└── src/
    └── main.rs     # complete single-file core engine + unit tests
```

---

## License

Released under the **MIT License** — see [LICENSE](LICENSE).

**CLDR** © Dr. Sohil Momin.
