<!--
  CLDR — Command Line Dispatch & Route  ·  CONTRIBUTING
  © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
  MIT-licensed: retain this attribution notice in all copies of this file.
-->

<div align="center">

# 🤝 Contributing to CLDR

[![Build & Release](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml/badge.svg)](https://github.com/drsamonline/cldr/actions/workflows/build-release.yml)
![Rust](https://img.shields.io/badge/rust-edition%202021-orange?logo=rust&style=flat-square)
![Style](https://img.shields.io/badge/style-rustfmt%20--edition%202021-blue?style=flat-square)

*← [README](README.md) · [Setup Guide](docs/SETUP_GUIDE.md) · [User Guide](docs/USER_GUIDE.md)*

</div>

---

## Ground rules

1. **Keep it lean.** CLDR's whole point is zero-bloat: no Electron, no Qt/GTK,
   no async runtime, single binary, <10 MB RAM. New dependencies need a strong
   justification in the PR description.
2. **Attribution watermark is mandatory.** Every file you add — source, doc,
   script, asset, workflow — must carry the banner:

   ```text
   © 2026 Dr. Sohil Momin (drsamonline). MIT licensed —
   attribution notice must be retained in all copies.
   ```

   Use the comment syntax native to the file type (`//`, `<!-- -->`, `#`,
   `::`). PRs that strip or omit watermarks will be closed.
3. **CI must stay green** — `cargo build --release --locked` and
   `cargo test --release --locked` on both Ubuntu and Windows runners.

## Development workflow

```bash
git clone https://github.com/drsamonline/cldr.git && cd cldr
rustup component add rustfmt clippy
cargo run                          # foreground TUI
cargo test                         # engine unit tests
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

### Module map

| File | Responsibility |
|---|---|
| `src/main.rs` | CLI router only — parse flags, dispatch to modes. No logic. |
| `src/engine.rs` | dispatcher, calculator, cascade, unit tests |
| `src/daemon.rs` | detached background worker, PID/heartbeat/stop sentinels |
| `src/tui.rs` | ratatui/crossterm UI + summon watcher wiring |
| `src/summon.rs` | clipboard mailbox, window raise, autostart, tray loop |
| `scripts/` | OS shortcut installers (PowerShell / POSIX sh) |
| `docs/` | user/setup/shortcut documentation |

## Submitting a pull request

1. Branch from `main`: `git checkout -b feat/<short-name>`.
2. One logical change per PR; update `CHANGELOG.md` under *Unreleased*.
3. Add/adjust unit tests for anything touching `engine.rs`.
4. Watermark every new file (see ground rule 2).
5. Ensure your commit tree contains **no** `target/`, logs, or OS junk
   (`.gitignore` covers these — don't weaken it).
6. Open the PR against `main`; CI runs automatically. Maintainer review required.

## Reporting issues

Use the [issue tracker](https://github.com/drsamonline/cldr/issues) with:
OS + DE/terminal, `cldr --version` output, exact keystrokes/command, expected
vs actual behaviour, and `cldr --daemon-status` output when the background
process is involved.

## Releasing (maintainers)

See §9 of the [Setup Guide](docs/SETUP_GUIDE.md#9--releasing-maintainers):
bump `Cargo.toml`, log it in `CHANGELOG.md`, tag `v<version>` — the tag/version
guard in [`build-release.yml`](.github/workflows/build-release.yml) rejects
mismatches before binaries are built.

---

<div align="center">

**© 2026 Dr. Sohil Momin ([@drsamonline](https://github.com/drsamonline))** —
MIT licensed · attribution watermark must be retained in all copies.

</div>
